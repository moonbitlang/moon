# Contributing Quick Start

## Developer workflows

- [Host handles](host-handles.md): registration, validation, owning tables, and
  operation-specific ownership rules.
- [Current executable path semantics](current-exe-semantics.md): load-time path
  capture, mutation behavior, executable-directory lookup, and guest-visible
  errors for file-backed and in-memory modules.
- [SQLite jobs](sqlite-jobs.md): SQLite payloads in the shared async pool,
  result ownership, asynchronous discard, and upstream Wasm wrapper integration.
- [Porting `moonbitlang/async` changes](async-upstream-porting.md): audit the
  one-way upstream runtime-port request queue, preserve exact provenance,
  deliver one upstream port at a time, and mark the originating async pull
  request completed after the Moon change lands.

## V8 value imports

The imports are grouped by their guest ABI:

- `filesystem/v8/values.rs` registers the string/array helpers alongside the
  other `__moonbit_fs_unstable` imports.
- `v8/ffi_bytes.rs` implements `ffi-bytes` and owns its memory registration.
- `v8/exception.rs` implements the exception tag and throw import.

These are synchronous Rust entry points. The shared callback adapter translates
Rust errors into V8 exceptions and preserves an existing V8 exception or
termination. Import setup does not compile JavaScript source. The WebAssembly
tag, exception, and memory constructors still use V8's native builtin API because
the pinned Rust bindings do not expose equivalent dedicated constructors.

Strings preserve UTF-16 code units, including unpaired surrogates. Builders own
private V8 ArrayBuffers with geometric growth; readers retain their live source.
The buffer/source and position are direct GC-traced fields of opaque, branded
objects. Rust uses temporary typed views and owns no persistent builder/reader
payload. Finishing a builder creates an independent value. The exception adapter
retains its constructor and tag through strong V8 handles for the duration of a Run.
Byte slicing preserves relative/clamped indices, copying permits overlapping
views, and `ffi-bytes.memory` is reacquired after growth. Byte views may only be
borrowed after argument conversion, with no further V8 calls during access.

The Wasm boundary tests in `tests/v8_native_imports.rs` cover these contracts,
catchable failures, and exception-tag identity. The value-helper GC test exercises
growth and collection with builders/readers retained only by Wasm locals/globals.
Removing the old JS throw helper
also leaves one more slot for guest frames in V8's default stack-trace limit.

## Worker cancellation

The cancellation protocol follows upstream async #595. Workers acknowledge a
cancellation request before entering native's cancellable syscalls. A Unix
signal arriving after that check may precede the syscall, so the signal handler
records the Job ID for a cancellation retry. The MoonBit
event loop calls `cancel_worker_with_retry` both to start cancellation and to
check each job notification. The host retries cancellation when needed and
returns a distinct status once the Job result is ready.

Cancellation eligibility also follows native. In async's
`src/internal/event_loop/io.mbt`, positioned reads and writes are submitted with
`cancellable=platform is Windows`. On Unix, cancellation can prevent assignment
to a Worker, but once assigned the guest waits for the operation to finish.
Accordingly, `pread` and `pwrite` have no cancellable region, as in native
`thread_pool.c`.
Windows positioned I/O is cancellable and has the corresponding region.

The Worker handle and its thread share one allocation containing scheduling
and cancellation state. Each executing Job borrows that state through a stack
context with an immutable `WorkerCompletionId`. A private scope guard borrows
the context, registers it in thread-local storage, and clears that binding
before the context leaves the stack. A Worker executes one Job at a time, and
each syscall region must end before another begins; assertions reject nesting
without replacing the active context or clearing its region mark.
`with_cancellable_region` borrows this context only for its synchronous closure,
so syscall regions cannot escape or retain a Worker. The closure and cancellation
check return errors through the same `AsyncHostResult`.
The region mark remains atomic for access by the interrupting signal handler;
cancellation status and retry mode remain atomic for access by the requesting
thread. No reference counts are changed when entering or leaving a region.

The caller supplies the Worker with a Job runner and a result sender. After the
runner returns, the Worker sends the host-owned Job result, sets `Waiting`, and
notifies the guest. This follows native's result-before-`Waiting` ordering. The
result channel transfers Rust ownership within the same process without copying
result buffers. Before accepting completion, the host restores the result to its
Job table. The guest wrapper copies output into Wasm memory when it subsequently
requests that output.

Unix notification transport deliberately differs from native's pipe of IDs.
Ordinary completion IDs stay in a host FIFO queue. Each Worker that enables
retries gets a preallocated atomic slot, so repeated retries for its current
Job coalesce without locking or allocating in the signal handler. A nonblocking
pipe carries only wake bytes, shared by pending notifications. A full wake
pipe already provides readiness; it cannot block a publisher or discard an ID.
The notifier retains both pipe ends while publishers are alive, even if the
guest closes its duplicate reader. Windows retains its existing IOCP delivery.

`fetch_completion` returns the same Job IDs through the existing guest import.
It clears the old wake before inspecting pending IDs, so a concurrent publisher
either joins that fetch or creates a new wake. Partial fetches restore readiness
on the level-triggered completion source. Only pending cancellation retries
scan registered retry slots; normal completion delivery does not scan Workers.
Run signals retain their separate coalesced source from the signal backport.

The historical `thread_pool/cancel_worker` import returns `RetryLater` for a
pending default Unix cancellation, so older guests retry after a timer. The new
`thread_pool/cancel_worker_with_retry` import opts into retry notifications and
combines native's cancellation call and completion check in one Wasm import.
It returns `0` for `RetryLater`, `1` for `NeedWait`, or `2` when the Worker is
`Waiting` and its result has been restored to the Job table. A Worker that has
acknowledged cancellation but not yet published its result still returns `1`.
The guest uses `2` to accept completion; the initial cancellation path still
waits for the completion notification to retire the Worker, as native does.
Older guests have no retry check and would treat retry notifications as
completed Jobs. Both imports take one `i64` Worker handle and return an `i32`
status; the historical import retains its original return values.
Internally, `CancellationOutcome` names these scheduler actions and is encoded
only when returning through the guest import. The separate internal state
`CANCELLATION_ACKNOWLEDGED` does not mean the Job has finished.

Switching a Job from `cancel_worker_with_retry` to legacy `cancel_worker`
disables further retry publication, but cannot retract a pending retry or one
already being published by a signal handler. A guest that has enabled retries
must still check completion before accepting those notifications.

On Unix, a retry check requires the registered completion source to match the
source retained by the Worker at spawn. Closing or replacing that source makes
the check return `Badf`, even for a finished Job. Results remain readable, and
legacy cancellation and freeing remain available.

The completion check describes the Worker's current Job; the import takes no
Job ID. Async's `EventLoop::handle_completed_job` accepts the previous completion
before calling `worker.wake` for the next Job. Pending Jobs stay in the guest's
queue until then. This sequencing keeps the checked Worker state associated
with the notified Job. The host also preserves an early wake from direct
callers, as tested by `wake_during_running_job_is_not_lost`, but after the Worker
advances, its cancellation status describes the new Job.

Worker freeing follows native's termination wakeup and join. Internally, Job
submission and stopping are separate operations: stopping returns any queued
Job and permanently closes admission, while the active Job can finish. A later
submission returns the unaccepted Job to its caller. Joining consumes the Rust
Worker handle, so there is no usable handle without an owned thread. Pool
completion publication is independent of guest consumption. Cancellation after
a Run stops executing its guest event loop remains a correctness FIXME for
teardown: a signal arriving before a blocking syscall may still require another
attempt, and no guest remains to request it. `free_worker` has no cancellation
retry loop and cannot forcibly stop noncooperative computation.

## How to Build and Test

```bash
cargo build
cargo test
cargo test -p moonrun --no-default-features --features wasmtime
```

## Before PR

We encourage to add the following prefix to your commit message and PR title: feat, fix, internal, or minor.

It's recommended to run the following command before you submit a PR, which may help discover some potential ci failure ASAP

```bash
cargo fmt

cargo clippy --workspace --exclude moonrun --all-targets --all-features -- -D warnings
cargo clippy -p moonrun --all-targets -- -D warnings
cargo clippy -p moonrun --all-targets --no-default-features --features wasmtime -- -D warnings

cargo test
```

We use [typos](https://github.com/crate-ci/typos) to avoid potential typos, you can also download and run it locally before PR.
