# SQLite jobs

The SQLite job payloads provide the Wasm counterpart of
[sqlite3.mbt PR #23](https://github.com/moonbit-community/sqlite3.mbt/pull/23),
reviewed at commit `70c02598aa924fa0bb7c6cedd75dc74d3af00657`. Native retains its dedicated C
executor. The Wasm wrapper privately adapts
SQLite jobs to the existing async thread pool; that pool supplies execution
mechanism, while SQLite owns operation semantics and result lifetimes.

## Ownership and execution

The Async Host owns Job Handles, scheduling, Workers, Completions, and Job
destruction. The SQLite Host owns SQLite inputs, captured results, and lifetime
leases. There are no SQLite executor Handles or SQLite-specific host threads.
The guest uses a FIFO async Mutex per connection to order its operations. All
domains use the same host Worker machinery; guest libraries can supply their
own completion pipe without accessing async's private scheduler.

Job creation copies filenames and UTF-16 SQL out of Guest Memory. A prepare,
step, or finalizer Job pins its Database until destruction, including when a
worker retains a detached Job. Step also pins its Statement. Closing a pinned
Database or accessing a pinned Statement traps. Synchronous operations on other
Statements remain available and may block on SQLite's connection mutex.
Guest mutex entries must be balanced before creating Jobs.

Statement lookup only validates liveness and rejects conflicting Job use; it
does not acquire the connection mutex. That exclusion also keeps statement-owned
column buffers stable during one synchronous host call. SQLite's own locking
handles synchronous calls that need serialized connection access.

Explicit host guards cover connection-field reads (`errcode`,
`extended_errcode`, and `changes64`, whose SQLite implementations do not lock)
and copying borrowed error-message memory. These guards prevent host data races
and invalid memory access. They do not associate a later synchronous error read
with an earlier call: intervening operations may replace that connection state.
A guest needing a `step`/error-read sequence uses the exposed Database Mutex
around the whole sequence. Untrusted call ordering need not preserve a guest's
intended result, but it must not violate host memory safety or resource lifetime.

Workers hold that mutex through the SQLite operation and capture of diagnostics
and change counts. Getters read the Job's owned snapshot, so later operations
cannot replace an earlier result. A successful open or prepare transfers its
Database or Statement exactly once. After transfer an open Job no longer owns
or refers to its Database; prepare keeps its Database lease until Job release.

SQLite Jobs follow the common release lifecycle. Freeing a Job while a worker
owns it detaches the Handle without cancelling execution. Unclaimed results
and unrun finalizers are destroyed with their Job, before its lifetime lease is
released. This fallback may close or finalize on the guest thread and block.
The MoonBit wrapper uses explicit asynchronous discard to keep normal cleanup
off the event loop. It checks cancellation and SQL-tail validity before taking
the resource. Rejected results move directly into a discard Job, without
publishing a Database or Statement Handle. The source result snapshot remains
readable and the discard Job retains its lifetime lease.

An ordinary finalizer constructor validates its Database and Statement before
consuming the Statement Handle. Discard consumes only a completed Job's owned
result. Discard returns the Runtime's null Handle when no unclaimed resource
remains. There is no public cleanup reservation or later statement binding.
Job construction and submission to an existing Worker need no fallible
OS-resource allocation; allocator exhaustion retains the existing Rust
allocation-failure behavior.

Cancellation keeps the existing finish-then-cancel contract. The wrapper shields
the wait, checks cancellation outside that shield, and shields required discard
before propagating cancellation. It does not request SIGUSR2 interruption or
call `sqlite3_interrupt`. A connection remains in flight through disposal.

The companion native implementation makes the same ownership decisions behind
its dedicated executor. Open and prepare jobs embed a second private queue node
and one-shot completion for discard. Their constructors initialize both pipe
notifications before submitting work, so disposal needs no later allocation.
The worker publishes result memory before closing its notifier; EOF wakes the
waiter, which acquires that publication. Accepted results only release metadata
and unused notification resources. Ordinary native finalization allocates and
submits in one call and leaves the statement with the caller if submission
fails. These details do not expose the async pool to native SQLite.

Runtime teardown first releases guest mutex entries. Its field destruction
order then joins and destroys Async Host workers and Jobs before destroying
remaining SQLite Statements and Databases. This also handles a guest that
traps or abandons Handles. Job leak accounting belongs to the Async Host.

## Wasm representation

[The shared import registry](../../src/sqlite/wasm/registry.rs) declares the same
nine SQLite job imports for V8 and Wasmtime: constructors for open, prepare,
step, finalize, and discard; result copying; handle transfer; and UTF-16 message
length and copying. Job Handles use the existing async Job kind in the Runtime's
shared namespace. Scheduling, return status, OS errors, completion, and release
use the existing `moonbitlang/async` thread-pool imports.

The pool's `ret` contains the SQLite status; its `err` remains the OS-error
channel. `sqlite3_job_result` copies a 24-byte little-endian record:

| Byte offset | Type | Value |
| --- | --- | --- |
| 0 | i32 | SQLite status |
| 4 | i32 | Extended SQLite status |
| 8 | i32 | Prepare tail, or -1 without a Statement |
| 12 | i32 | Whether the affected-row count is present |
| 16 | i64 | Affected-row count, or zero when absent |

The async prepare tail is relative to the copied SQL view, matching native
async prepare. Synchronous prepare instead returns an absolute offset in the
backing String. Diagnostics use UTF-16 length-and-copy imports and are empty
for successful Jobs. Invalid Handles, sequencing, and memory arguments trap;
output buffers are validated before result copying.

## Worker completion pipes

Libraries can await host Jobs without accessing async's private scheduler:

```text
// Imports under moonbitlang/async; i64 values are opaque Runtime Handles.
spawn_worker_with_pipe(completion_id: i32, job: i64, writer: i64) -> i64
wake_worker(worker: i64, completion_id: i32, job: i64) -> void  // unchanged
```

The writer must be an async write end from `fd_util/pipe`, as returned by
public `@pipe.pipe()`. Validation happens before consuming the Job. The Worker
retains the writer for its lifetime, so the guest can close its writer Handle
after spawning. All subsequent Jobs use the same pipe. This path does not
require `init_thread_pool`; ordinary `spawn_worker` keeps its default notifier.

After publishing each Job result and marking itself Waiting, the Worker writes
its completion ID as four little-endian bytes. Unlike the default Unix notifier,
whose pipe carries only wake bytes for a host completion queue, this pipe carries
the IDs themselves. The reader accumulates a full record and routes the ID to
its awaiting coroutine. The guest reads `job_get_err` / `job_get_ret` before
operation-specific results, following the existing Job protocol. Those common
status getters and SQLite's existing Job-access helpers collect completed
results without requiring the private `fetch_completion` path.

The reader must keep draining while Workers are active. Freeing a Worker or
tearing down the Runtime interrupts notification waits even if the pipe is
full, then releases the retained writer. EOF is not a successful completion.
As with the default notifier, a closed reader does not retire the Worker or
change Job ownership; the guest still releases both. On Windows private
writes use overlapped I/O without posting their own packets to the guest IOCP.

These records report finished Jobs only. `cancel_worker_with_retry` is reserved
for async's default completion source and is rejected for Workers with supplied
pipes, since their IDs cannot safely enter async's private cancellation queue.
External callers can use legacy `cancel_worker` and arrange its requested
retries themselves. SQLite keeps its shielded finish-then-cancel contract and
does not interrupt the Worker.

The Runtime continues to own Worker execution and teardown. Choosing a pipe
adds no scheduler or thread limit and does not promise an OS-thread identity.

## Guest integration and coverage

The upstream SQLite Wasm wrapper must enable its shared async code, represent
its executor as an async Mutex, and lower its operation-specific Job wrappers
to these imports. It can create a Worker with a public async pipe and await
completion records through `@io.Reader`, without a public `perform_host_job`.
The caller retains ownership and frees the Job after taking or discarding owned
results, or copying scalar results.

The integration harness uses a disposable copy of the pinned async source
without modifying its scheduler or exports. The MoonBit fixture uses async tests
for SQL views, captured diagnostics, discard, cancellation, event-loop progress,
and SQLite ordering alongside filesystem work. Its convenience helper creates
a Worker per operation; a separate case reuses a Worker after closing the guest
writer Handle, including after Windows IOCP registration. The test runner owns
the event loop. The harness runs all cases through `moon test` with its moonrun override
and leak checking enabled. The engine ABI regression additionally checks EOF
after Worker release and result access without an async event loop. Host tests
cover invalid writer admission, closed readers, full-pipe teardown, and legacy
cancellation without leaking retry IDs into the default completion source.

`src/sqlite/jobs/tests.rs` covers worker reuse, pinned lifetimes, detached and
unclaimed result cleanup, single-transfer results, discard without publishing
handles, invalid finalizer inputs, and Runtime teardown.
`src/sqlite/wasm/context.rs` checks invalid guest memory without consuming Job
results.
