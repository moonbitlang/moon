# Contributing Quick Start

## Developer workflows

- [Runtime architecture](design/runtime-architecture.md): read before changing
  embedding interfaces, host imports, virtualized state, or authorization. It
  defines ownership, the three host-operation layers, authority mapping, and
  current enforcement gaps under [ADR 0012](../adr/0012-preserve-embeddability-and-runtime-owned-host-semantics.md).
- [Architecture tracking](architecture-tracking.md): the current ownership and
  execution milestone after recursive virtual process trees, and deferred work.
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

## Shared Wasm import registration

`wasm_imports::ImportRegistry` defines synchronous imports once for V8,
Wasmtime and wee8. WASIp1, async, SQLite, core executable paths, and the legacy
filesystem/environment/value/stdio/random/time/process imports supply shared
registrations. Backend setup supplies per-run state, memory access, native value
representation and callback retention.

Registration follows Wasmtime's [`func_wrap`/`IntoFunc` pattern](https://github.com/bytecodealliance/wasmtime/blob/v48.0.1/crates/wasmtime/src/runtime/func.rs):
ordinary typed Rust callbacks supply the parameter types and order. The Rust
return type supplies one numeric or nullable externref result, or no result. The internal tuple
connects argument decoding to invocation; definitions take individual arguments
and may capture owned state. `HostFunction` supports zero through eleven arguments,
including async's process-spawn and TLS signatures. It does not reuse Wasmtime's
engine-specific host-function representation.

All adapters decode parameters before borrowing Guest Memory. Integers cross
as signless bits; floats use their bit representation. V8 converts i64 through
BigInt, never through a JavaScript Number. This preserves host Handles and
SQLite integer values outside the exact f64 integer range. Reference parameters
and results borrow the current native invocation. V8 uses Local handles,
Wasmtime uses its host trampoline's root scope, and wee8 uses its store-owned
native roots; Moonrun adds no integer reference table. Multi-result host calls
remain outside this interface.

Reference signatures rebind the call lifetime through the internal type traits.
Named functions spell that lifetime explicitly; captured reference closures use
small higher-ranked signature helpers so Rust can infer the relationship. The
reference signatures cover the current legacy ABI. Adding a different shape
requires extending that internal signature list, not editing each adapter.

The caller exposes scoped Host Domain state, reference conversion and a run
termination request. `with_memory` consumes it and passes a `MemoryCaller`
without engine access. Host code therefore cannot allocate references or re-enter
the engine while a guest memory view is live. The separate context and reference
lifetimes allow a result root to outlive the temporary adapter borrow until the
engine has encoded that result. A memory-free
operation can run without exported memory, including in a module start section.
Each domain interprets memory-access failure as before: WASI returns EFAULT for
unbound V8 memory, async records Fault when its implementation needs memory, and
SQLite memory operations trap with Fault. Wasmtime supplies an empty slice
without an exported memory; wee8 cannot obtain memory during a start section.
This registration change does not add load-time memory validation. Memory is
reacquired each call and released before returning or interrupting the engine.

Async keeps its provenance-bearing declaration list and produces one set of
typed registrations. SQLite definitions call the existing Host Domain methods
or construct the shared checked memory context. Errors retain their import name
and domain error; platform-inactive async definitions produce a host error if
called. No engine-specific context or return-conversion macro remains in either
host family.

`tests/wasi_imports.rs` and `tests/numeric_host_imports.rs` run identical modules
on all three selected engines. They cover import order, numeric values and
Handles, memory growth, invalid memory and handles, termination during
instantiation, and worker completion/teardown. Compiler-produced async and
SQLite CLI fixtures exercise the same registrations, including filesystem
policy, native resource cleanup, and i64/f64 database values.

## Synchronous jobs

`thread_pool/run_job(i64) -> void` runs one fresh Job synchronously on the
calling thread. It uses the same domain authorization, path interpretation,
executor, result envelope and resource publication as worker execution. It does
not require a thread pool or emit a worker Completion. The caller checks
`job_get_err`, reads `job_get_ret` and operation-specific results, then calls
`free_job`. A Job instance can be executed only once; reuse the constructor and
executor, not the completed Job.

Synchronous execution blocks the calling thread. Ordinary worker cancellation
regions have no Worker state on this path; jobs with a dedicated cancellation
hook retain the existing Run Termination wake binding. Use workers when coroutine
progress or worker cancellation is needed. The runner itself does not borrow
Guest Memory, but constructors and result getters may. In particular, generic
stat still needs Guest Memory for copy-out, so the scalar `stdio/kind_of_fd`
import remains necessary for stdio initialization during Wasm startup.

## Async import context

Async FFI implementations use one engine-independent `ImportContext`: borrowed
Async Host and Host Filesystem references, Guest Memory, and a termination
request. Backend adapters decode arguments before borrowing memory and release
the borrow before converting results or interrupting execution. The context
cannot re-enter the guest or retain an engine scope or memory binding.

Memory access failures are deferred until an implementation requests memory,
so memory-free imports also work during instantiation and without an exported
memory. V8 reacquires the current backing buffer for each import; Wasmtime
borrows its caller's memory and store together. A termination import records
its outcome; the backend interrupts guest execution after the borrow ends.
Core executable-path imports access Runtime directly from shared registration
rather than exposing Runtime through the async context.

## V8 value imports

The imports are grouped by their guest ABI:

- `host_imports.rs` registers the legacy host ABI once for all adapters;
  `v8/values.rs` supplies V8's native string/array/builder/reader representation.
- `wasm_imports/payload.rs` shares the typed Rust payload representation used by
  Wasmtime and wee8. String conversion belongs to each adapter: Wasmtime uses
  UTF-16 payloads, while wee8 uses native V8 strings. Both share byte/helper
  payloads. A guest GC object without a Rust payload is a valid
  externref but is not a host string.
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
payload. Finishing a builder creates an independent value. Opaque RNG and clock values use branded V8 objects with native weak finalizers;
the weak registrations own cleanup closures without keeping guest objects alive.
Run teardown also releases payloads that have not yet been collected. The exception adapter
retains its constructor and tag through strong V8 handles for the duration of a Run.
Byte slicing preserves relative/clamped indices, copying permits overlapping
views, and `ffi-bytes.memory` is reacquired after growth. Byte views may only be
borrowed after argument conversion, with no further V8 calls during access.

The Wasm boundary tests in `tests/v8_native_imports.rs` cover these contracts,
catchable failures, and exception-tag identity. The value-helper GC test exercises
growth and collection with builders/readers retained only by Wasm locals/globals.
Removing the old JS throw helper
also leaves one more slot for guest frames in V8's default stack-trace limit.

## Wasm diagnostics

V8, Wasmtime and wee8 send guest frames through `wasm_diagnostic::render`. All
adapters use the numeric module offset for source-map lookup. V8 also supplies
its original runtime location, which includes the offset for Wasm frames and
may instead name a host location. Feature guards select V8's full location or
Wasmtime/wee8's numeric offset for display.

Each frame shows the mapped source file and line first, followed by the runtime
location in parentheses. Missing or malformed source maps do not erase the
runtime information needed to investigate a failure. `--no-stack-trace` still
suppresses all frames.

## wee8 runtime integration

wee8 enables native JS-string builtins and imported string constants during
compilation. The module retains that configuration through serialization; the
Rust host registry sees only non-builtin imports. Runtime compatibility imports
(`ffi-bytes`, console and memory sanitizer) live in `wee8/runtime_imports.rs`,
separately from shared Host Domain registrations. Byte memory is store-owned;
its closure borrows cannot overlap growth or guest execution.

Shared callbacks still use `Option<ExternRef>`. The adapter checks that guest
`ValType`s fit the declared Rust value representation, constructs each host
function with the guest's exact nullability,
and relies on the upstream safe API to reject null non-null results. It does
not change shared host definitions or relax V8 import matching. String helpers
construct native V8 strings, including test-driver filenames.

Native frame snapshots are rendered through the shared Wasm diagnostic module,
using retained Wasm function names and the existing source map. A stack-limit
guard checks the owning thread's actual stack bounds and restores the previous
limit before store teardown. It does not allocate a replacement thread stack.

## External Run termination

Guest exits and external requests share one latched outcome. Requests,
interrupt binding, and finalization use the same lock; accepted termination wins
until finalization closes the channel after teardown. V8 enables interruption
only during guest execution and defers it across host conversions and Guest
Memory borrows. A host-only poll wakeup remains pending across wait entry and
is detached before the wait returns; it is never delivered as a guest signal.
See `run_termination.rs` for the control contract and
`tests/run_termination.rs` for timeout-bounded lifecycle tests.

## Virtual process supervision

`VirtualProcessTree` admits Supervised Commands under stable virtual PIDs. A
command includes preparation, a Wasm Run, and command-owned cleanup. Guests can
signal themselves or children and wait for direct children through existing
async APIs. Unix supports
SIGINT/SIGTERM/SIGHUP, SIGKILL and the signal-zero probe; Windows uses Ctrl-Break
for graceful cancellation and exit code 1 for hard cancellation. Unhandled soft
signals terminate only the target Run; accepted signals leave the grace period
to the guest caller. Windows handles retain the exit status after reaping.
`spawn_moon` asks a host-selected Moon/moonx helper to hand off Wasm, source map,
arguments, cwd and optional policy, then executes it under the allocated PID.
The command execution module owns preparation and artifact dispatch. Its
private delivery directory stays alive through Runtime teardown; normal build
caches stay with Moon. The supervisor retains the sole termination receiver
until command cleanup finishes. `Engine::run_in_scope` borrows that control and
cannot close it when the Wasm Run returns. Standalone `Engine::run` still owns
and closes its own receiver. An accepted termination overrides a normal result
or execution error until the owning scope finalizes; panics remain process
failures. Completion is published only after thread reaping.

`RuntimeConfig` carries the single set of environment, cwd, stdio, policy and
process configuration inputs from admission through preparation to Runtime
construction. Helpers borrow these inputs without taking Run control or
executing guest code. Runtime realizes the canonical state once; configuration
does not itself grant host access. Helpers retain host access. Cancellation
during preparation waits for helper exit and prevents Wasm startup; helper-group
cancellation remains a FIXME. The Run's Host Process checks guest spawn policy,
resolves Moon/moonx against its environment and cwd, and prepares owned stdio,
environment and policy for tree admission. Descendants join the same tree;
policy environment provisioning reads the supplied child environment.
Other native guest spawning remains unsupported in this mode.
Ordinary Runs remain ambient. The tree owns
PIDs, child relationships, and one reaper per Run, so blocked thread cleanup
cannot delay other completions. A parent Run owns its wait Jobs and poll
registrations. On macOS, its kqueue notifier observes tree completion through a
weak `Waker`; the tree neither owns the parent poll nor knows its transport.
Guest wait Jobs
are cancellable and only consume the result when the guest reads it. Dropping an
unread Job preserves that result. Exiting parents leave children with the
supervisor. The CLI's `--virtual-process-tree` mode waits for adopted descendants
before exiting with the root outcome. `Terminating` does not promise bounded
teardown. See `tests/process_tree.rs`, `tests/guest_process_control.rs` and
`tests/recursive_moon.rs`.

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
completion publication is independent of guest consumption. During teardown,
`shutdown_workers` closes admission, preserves queued Jobs, and drives
cancellation retries for all Workers before joining any. Retries use a 10 ms
timer without guest polling. Joining still requires actual thread exit;
non-cancellable host operations cannot be forcibly stopped. The native-shaped
`free_worker` remains a plain termination wakeup and join.

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
