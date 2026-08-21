# Moonrun

Moonrun executes MoonBit wasm programs and provides host services that wasm code cannot perform directly.

## Language

**Job**:
A host operation requested by guest code whose result is observed later by the guest coroutine.
_Avoid_: Task, request

**Worker**:
A host execution unit that runs a job outside the guest coroutine loop.
_Avoid_: Executor thread, background task

**Completion**:
The host-owned result of a finished job that is ready to wake or resume guest code.
_Avoid_: Callback, event

**Completion Queue**:
A host-owned queue of completed job identifiers that the guest event loop drains to resume waiting coroutines.
_Avoid_: Notify pipe, callback queue

**Guest Memory**:
The wasm linear memory owned by the guest program. A runtime adapter exposes it
to a host operation only through a fresh, checked borrow for one host call.
That borrow uses unsigned wasm addresses, does not reserve address zero, and
must not survive guest re-entry or memory growth; an individual ABI decides
whether zero represents null.
_Avoid_: Wasm buffer, V8 Memory Binding, retained memory pointer, C null policy

**Untrusted Guest**:
A wasm program that may call the async boundary outside the sequencing and ownership discipline expected from MoonBit async code.
_Avoid_: Random wasm, malicious MoonBit

**Guest String Path**:
A MoonBit `String` pointer plus UTF-16 code-unit length used for async path arguments crossing `moonbitlang/async`.
Moonrun converts this directly into `OsString`; guest code must not send UTF-8 `Bytes` for paths.
_Avoid_: Guest UTF-8 path buffer

**Host Buffer**:
Memory owned by moonrun while servicing guest jobs.
_Avoid_: Native buffer, temporary buffer

**Moonrun Policy**:
The permission configuration enforced by moonrun-owned host imports, including
`moonbitlang/async` and `__moonbit_fs_unstable`. It authorizes host paths and
operations before moonrun performs them. It does not implicitly configure or
restrict WASI descriptors.
_Avoid_: WASI sandbox, virtual filesystem

**WASI Capability Surface**:
The files and directories reachable through WASI descriptors and preopens.
WASI access is configured separately from Moonrun Policy, even when both
surfaces ultimately access the same host filesystem. A runtime adapter must
configure both explicitly instead of treating one as enforcement for the
other.
_Avoid_: Moonrun Policy, FFI permissions

**Host Filesystem**:
The runtime-engine-neutral implementation of moonrun's permission-backed
filesystem imports. It owns authorization, host filesystem operations, and
guest-visible error semantics; runtime adapters only convert values and expose
imports. WASI does not pass through the Host Filesystem.
_Avoid_: WASI filesystem, V8 filesystem

**Handle**:
An opaque value held by MoonBit code that names a moonrun-owned object, such as a Resource, Job, Worker, poll instance, Host Buffer, address-info result, Completion Source, SQLite Database, or SQLite Statement.
_Avoid_: Host Handle, Guest Handle, raw fd, pointer, id

**Host**:
The per-run composition root for moonrun-owned import state. It creates one Host Key namespace, wires it into domain modules such as the Async Host and SQLite Host, and performs leak checking only when the complete run is torn down. Domain operations and payload accounting remain on their owning Host module.
_Avoid_: giant host API, async-only host

**Host Key**:
The internal generational key behind a Handle. One primary Host Key table records only liveness and resource kind; domain payloads live in secondary maps keyed by Host Key.
All Handle kinds share the slotmap null Host Key. An ABI that needs to create or compare a null Handle obtains its encoded value from the running Host rather than hard-coding the slotmap representation.
_Avoid_: resource payload, raw pointer, per-API key

**V8 Memory Binding**:
The V8 adapter's retained handle to the instance's exact exported memory. The JavaScript runner binds it once after instantiation, and every memory-consuming import reacquires the current backing buffer before borrowing Guest Memory. Imports invoked by a wasm start function cannot use this post-instantiation binding.
_Avoid_: Guest Memory, Host memory, generic runtime memory

**V8 Run Context**:
The V8-private per-run composition object that retains the Host, V8 Memory Binding, and termination request for current V8 adapters. Other wasm runtimes carry the same engine-neutral Host state through their own adapter mechanisms rather than implementing a universal runtime context.
_Avoid_: Host, import family, universal runtime context

**Resource**:
A moonrun-owned OS or runtime object that can be acquired by a Job, such as a file, socket, or directory cursor.
A Resource is not the Handle that names it.
_Avoid_: Capability, Host Resource, Guest Resource, raw fd, pointer, id

**Resource Class**:
The host-side classification of a Resource used for operation checks and future policy decisions; the current classes are file, TCP socket, and UDP socket.
A Resource Class is not a separate Handle namespace.
_Avoid_: Handle type, fd type, raw OS type

**Resource Handle**:
A Resource Handle is a Handle that names a Resource while it remains reachable to guest code.
Closing a Resource Handle removes future reachability; it does not describe ownership of already-acquired references.
_Avoid_: Host Handle, Guest Handle, raw fd, pointer, id

**Acquired Resource**:
A host-owned reference to a Resource captured before a Job runs.
It lets an already-submitted Job finish without duplicating OS handles, even if the Resource Handle is closed later.
_Avoid_: Duplicated fd, borrowed fd, guest handle

**Native-Shaped Async Boundary**:
The wasm async host boundary that keeps MoonBit-facing concepts aligned with `moonbitlang/async` native concepts even when moonrun uses different host representations.
_Avoid_: Wasm-specific async API, shortcut API

**Native Behavior**:
The observable behavior of `moonbitlang/async` native execution that moonrun should match byte-for-byte unless that behavior is questionable or not user facing.
For normal MoonBit async paths, moonrun should stay strictly native-shaped and avoid adding observable intermediate states. Extra validation exists at the async boundary to reject stale or unexpected calls from an Untrusted Guest before they can violate moonrun's Rust or OS ownership invariants.
_Avoid_: Conceptual parity, best-effort compatibility

**Run Termination**:
A per-Wasm-run outcome requested by guest code, either an exit code or termination by signal. A runtime adapter records Run Termination and interrupts guest execution without terminating its embedding process; only the outer CLI adapter applies the outcome after guest and host state have been torn down.
_Avoid_: Host exit, import-side exit, process-global termination state

**Loaded Module**:
A shareable, immutable guest program prepared once by one Engine. Multiple Runs
can execute one Loaded Module without preparing its source again. It carries
the backend state needed to create fresh guest execution state for each Run.
_Avoid_: Wasm bytes, Run, file path alias

**Engine**:
The shareable Moonrun execution interface that prepares reusable Loaded Modules
and executes Runs. It neither chooses execution placement nor retains Run
lifecycle state.
_Avoid_: Run, execution thread, thread pool

**Run**:
One synchronous execution of a Loaded Module using execution placement chosen
by the caller. A Run owns fresh guest and Host state and returns only after that
state is torn down.
_Avoid_: Engine, Worker, execution thread

**Async API**:
The V8-facing `moonbitlang/async` adapter that registers imports, decodes wasm ABI values, reacquires guest memory, sets return values, and reports traps.
_Avoid_: Runtime state, native-stub implementation

**Async Host**:
Moonrun-owned async runtime state for one `moonbitlang/async` host instance: Resources, host workers, completion queues, Jobs, and opaque host poll instances. It uses the Host's shared Host Key namespace and contains no SQLite state.
_Avoid_: `moonbitlang/async` source mirror

**SQLite API**:
The V8-facing `moonbitlang/sqlite` adapter that lowers SQLite-shaped calls into the portable wasm ABI, borrows Guest Memory for synchronous native calls, and reports ABI contract violations as traps. Native SQLite pointers never cross this interface: SQLite objects and the reserved VFS parameter use opaque `u64` Handles with one runtime-discovered null Host Key. UTF-8 filenames use a backing Bytes value plus its byte length; the adapter bounds the read by that length, validates the encoding and absence of interior NULs, then copies it into a NUL-terminated Host Buffer for SQLite. UTF-16 SQL uses a backing String plus code-unit offset and length; `pzTail` is returned as an absolute code-unit offset in that same String so a StringView can contain multiple statements. Bound UTF-16 and blob views use `SQLITE_TRANSIENT`, so SQLite copies them before the Guest Memory borrow ends. Borrowed error messages, column names, and text/blob columns use length-and-copy imports instead of exposing SQLite-owned pointers.
_Avoid_: SQLite Host, SQLite wrapper SDK

**SQLite Host**:
The engine-neutral SQLite implementation for one run. It owns SQLite policy and operations, uses the Host's shared Host Key namespace, and contains the Database and Statement pointer maps, teardown, and leak accounting. Runtime adapters lower their own memory and scalar representations before crossing its interface.
_Avoid_: SQLite API, Async Host, V8 SQLite

**Async Sys**:
The V8-free native-stub port layer. Implemented files follow the `moonbitlang/async` source layout and carry provenance for the native source path and symbol they track. Poller files are direct ports behind the wasm `poll/*` imports.
_Avoid_: V8 adapter, placeholder unsupported imports

**Host Poller**:
The `async_sys::internal::event_loop::poll` port of native epoll, kqueue, or IOCP. The wasm event loop owns opaque `Instance` handles and calls `poll/wait`, `poll/event_fd`, and `poll/event_events`. Resource registrations store their Resource Handle directly in the poller's opaque user-data field. The event accessor returns the native field unchanged, including platform-specific non-Resource values; later Resource operations—not the accessor—validate any returned Handle.
_Avoid_: Completion queue, worker wakeup

**Thread-Pool Completion Source**:
The host-side notify handle corresponding to `thread_pool.c`'s `pool.notify_send`. Worker threads write or post completed job ids through it so `poll/wait` reports the completion source key, after which MoonBit drains `thread_pool/fetch_completion`.
_Avoid_: Host Poller, Barrier, worker wakeup
