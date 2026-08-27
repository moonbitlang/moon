# Moonrun

Moonrun executes MoonBit wasm programs and provides host services that wasm code cannot perform directly.

## Language

**Job**:
A host operation requested by guest code whose result is observed later by the
guest coroutine. Its `err` field carries host or system errors handled uniformly
by the MoonBit worker loop. When `err` is zero, its `ret` field is defined by the
operation and may be a value, a success sentinel, or a domain-specific status;
structured results and domain-specific diagnostics remain in its payload.
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

**Runtime Configuration**:
Construction input that selects how one Runtime behaves, such as its Working
Directory, Env initialization, stdio bindings, or process implementation. It
does not grant permission to perform host operations.
_Avoid_: Moonrun Policy, Runtime State, global process configuration

**Runtime State**:
The canonical realized state owned by one Runtime, including Env, Runtime
Working Directory, stdio bindings, Handle namespace, and Host-domain state.
Every Host Domain that needs one of these dependencies observes the same
Runtime-owned state rather than independently reconstructing it.
_Avoid_: Moonrun Policy, Runtime Configuration, process-global state

**Moonrun Policy**:
Authorization input that answers which fully interpreted host operations are
allowed. It does not own or select Runtime State such as Env, Runtime Working
Directory, stdio bindings, VFS configuration, child tables, or OS handles.
Moonrun Policy does not implicitly configure or restrict WASI descriptors.
_Avoid_: Runtime Configuration, Runtime State, WASI sandbox, virtual filesystem

**Host Domain**:
The unique semantic owner for one family of host behavior. A Host Domain
interprets a request using its Runtime State and Handles, derives the complete
intent, authorizes it with its domain policy at the last safe moment, performs
the Raw Operation, and updates its state. The primary call chain is Wasm
Adapter → Host Domain → Raw Operation; Runtime composes this chain but does not
forward each call through another layer.
_Avoid_: Wasm Adapter, Raw Operation, forwarding service

**Raw Operation**:
A platform or library primitive below a Host Domain, such as an OS syscall or
SQLite FFI call. It implements mechanism after semantic interpretation and
authorization; it does not independently own Policy or Runtime State.
_Avoid_: Host Domain, Wasm Adapter, policy check

**Wasm Adapter**:
A backend-specific boundary that decodes guest representations, validates ABI
contracts, invokes the owning Host Domain, and encodes results. It does not own
domain semantics and must not bypass a Host Domain to call its Raw Operations.
_Avoid_: Host Domain, Raw Operation, Runtime

**Env**:
The environment interface owned by one Runtime and shared by MoonBit
environment imports, WASI environment calls, temporary-directory resolution,
and child inheritance. Provisioning rules may determine its initial contents;
after construction it is Runtime State. Ambient retains process-environment
write-through for guest-reachable effects that do not yet cross Env.
_Avoid_: Mutable policy, per-import environment map

**WASI Capability Surface**:
The files and directories reachable through WASI descriptors and preopens.
WASI access is configured separately from Moonrun Policy, even when both
surfaces ultimately access the same host filesystem. A runtime adapter must
configure both explicitly instead of treating one as enforcement for the
other.
_Avoid_: Moonrun Policy, FFI permissions

**Runtime Working Directory**:
The working-directory behavior owned by one Runtime and used by cwd-dependent
Host domains and backend adapters. The only current value is `Ambient`: it
preserves historical behavior by observing or inheriting the process current
directory at the same execution points as before this seam. It does not
snapshot a path, retain a directory handle, change the process cwd, or isolate
concurrent Runs. New modes belong behind this seam rather than in individual
filesystem, process, SQLite, policy, or WASI call sites.
_Avoid_: sandbox root, virtual filesystem root, captured cwd

**Host Filesystem**:
The Runtime-owned, backend-neutral implementation of moonrun's
permission-backed filesystem operations. It owns filesystem authorization,
Filesystem Job payloads, execution, and result interpretation. Wasm runtime
adapters only convert values and expose imports; the thread pool only schedules
Filesystem Jobs and delivers their Completions. WASI does not pass through the
Host Filesystem.
_Avoid_: WASI filesystem, V8 filesystem

**Host Network**:
The Runtime-owned, backend-neutral implementation of moonrun's
permission-backed network operations. It creates TCP and UDP Resources and
owns network authorization, synchronous socket operations, Network Job payloads,
execution, and result interpretation. The Async Host owns the Host Network,
Resource Handles, and asynchronous lifecycle; the thread pool only schedules
Network Jobs and delivers their Completions.
_Avoid_: V8 network, Async Host network

**Host Process**:
The Runtime-owned, backend-neutral implementation of moonrun's
permission-backed process operations. It owns Process Job payloads,
authorization and final configuration, result interpretation, and child-process
authority. Native execution stays behind its private Ambient Process
implementation. The Async Host owns guest Handles and worker lifecycle; the
thread pool only schedules Process Jobs and delivers their Completions.
_Avoid_: V8 process, thread-pool process

**Ambient Process**:
The private operating-system implementation inside Host Process. It preserves
native spawn, executable lookup, wait, cancellation, and error behavior while
hiding Unix and Windows details below the Host Process seam. It is not a
general process adapter interface; such an interface becomes useful only when
another execution implementation exists.
_Avoid_: Host Process, process adapter interface, virtual process

**Handle**:
An opaque value held by MoonBit code that names a moonrun-owned object, such as a Resource, Job, Worker, poll instance, Host Buffer, address-info result, Completion Source, SQLite Database, or SQLite Statement.
_Avoid_: Host Handle, Guest Handle, raw fd, pointer, id

**Runtime**:
The backend-neutral composition root and isolation boundary for one virtual
environment. It consumes Runtime Configuration and authorization inputs, owns
the canonical Runtime State and Host Domains, and wires each domain's
dependencies during construction. It is not a forwarding layer for individual
operations. A Runtime is distinct from the process-shared Engine, from a guest
Instance, and from the Run action that executes guest code. An Ambient
dependency may still deliberately expose process-global behavior.
_Avoid_: Host, Engine, V8 Run Context, service locator

**Host Key**:
The internal generational key behind a Handle. One primary Host Key table records only liveness and resource kind; domain payloads live in secondary maps keyed by Host Key.
All Handle kinds share the slotmap null Host Key. An ABI that needs to create or compare a null Handle obtains its encoded value from the running Runtime rather than hard-coding the slotmap representation.
_Avoid_: resource payload, raw pointer, per-API key

**V8 Memory Binding**:
The V8 adapter's retained handle to the instance's exact exported memory. The JavaScript runner binds it once after instantiation, and every memory-consuming import reacquires the current backing buffer before borrowing Guest Memory. Imports invoked by a wasm start function cannot use this post-instantiation binding.
_Avoid_: Guest Memory, Host memory, generic runtime memory

**V8 Run Context**:
The V8-private per-run adapter object that retains Runtime, V8 Memory Binding,
and the termination request for current V8 adapters. Other wasm runtimes carry
the same backend-neutral Runtime through their own adapter mechanisms rather
than implementing a universal runtime context.
_Avoid_: Runtime, Host, import family, universal runtime context

**Resource**:
A moonrun-owned OS or runtime object that can be acquired by a Job, such as a file, socket, or directory cursor.
A Resource is not the Handle that names it.
_Avoid_: Capability, Host Resource, Guest Resource, raw fd, pointer, id

**Resource Class**:
The host-side classification of a Resource used for operation checks and future policy decisions; the current classes are file, TCP socket, and UDP socket.
A Resource Class is derived from the Resource payload rather than stored as an independent tag.
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
The process-shared Moonrun backend that prepares reusable Loaded Modules and
executes Runs. It neither owns a Runtime's virtual environment nor retains Run
lifecycle state.
_Avoid_: Runtime, Instance, Run, execution thread, thread pool

**Instance**:
An instantiation of a Loaded Module inside a Runtime, with its own Guest Memory
and guest execution state. A Runtime may contain multiple related Instances; a
Run is the action that executes guest code rather than the Instance itself.
_Avoid_: Runtime, Run, Loaded Module, poll instance

**Run**:
One synchronous execution action within a Runtime, using execution placement
chosen by the caller. It returns only after run-local guest execution state is
torn down.
_Avoid_: Engine, Runtime, Instance, Worker, execution thread

**Async API**:
The V8-facing `moonbitlang/async` adapter that registers imports, decodes wasm ABI values, reacquires guest memory, sets return values, and reports traps.
_Avoid_: Host state, native-stub implementation

**Async Host**:
Moonrun-owned async state for one `moonbitlang/async` host instance: Resources, host workers, completion queues, Jobs, and opaque host poll instances. It uses the Runtime's shared Host Key namespace and contains no SQLite state.
_Avoid_: `moonbitlang/async` source mirror

**SQLite API**:
The V8-facing `moonbitlang/sqlite` adapter that lowers SQLite-shaped calls into the portable wasm ABI, borrows Guest Memory for synchronous native calls, and reports ABI contract violations as traps. Native SQLite pointers never cross this interface: SQLite objects and the reserved VFS parameter use opaque `u64` Handles with one runtime-discovered null Host Key. UTF-8 filenames use a backing Bytes value plus its byte length; the adapter bounds the read by that length, validates the encoding and absence of interior NULs, then copies it into a NUL-terminated Host Buffer for SQLite. UTF-16 SQL uses a backing String plus code-unit offset and length; `pzTail` is returned as an absolute code-unit offset in that same String so a StringView can contain multiple statements. Bound UTF-16 and blob views use `SQLITE_TRANSIENT`, so SQLite copies them before the Guest Memory borrow ends. Borrowed error messages, column names, and text/blob columns use length-and-copy imports instead of exposing SQLite-owned pointers.
_Avoid_: SQLite Host, SQLite wrapper SDK

**SQLite Host**:
The backend-neutral SQLite implementation owned by one Runtime. It owns SQLite policy and operations, uses the Runtime's shared Host Key namespace, and contains the Database and Statement pointer maps, teardown, and leak accounting. Wasm runtime adapters lower their own memory and scalar representations before crossing its interface.
_Avoid_: SQLite API, Async Host, V8 SQLite

**Async Sys**:
The V8-free native-stub port layer. Ports carry provenance for the native source
path and symbol they track. Reusable operating-system operations remain here;
Job implementations live with their owning domain. Poller files are direct
ports behind the wasm `poll/*` imports.
_Avoid_: V8 adapter, placeholder unsupported imports

**Thread Pool**:
The shared host facility that schedules Jobs outside the guest coroutine loop.
It owns the common Job result envelope, Workers, and Completion delivery. It
does not interpret Filesystem, Network, or Process Job semantics, which remain
in their owning domain modules.
_Avoid_: Filesystem executor, Network executor, Process executor, SQLite executor

**Host Poller**:
The `async_sys::internal::event_loop::poll` port of native epoll, kqueue, or IOCP. The wasm event loop owns opaque poll-instance Handles and calls `poll/wait`, `poll/event_fd`, and `poll/event_events`. Resource registrations store their Resource Handle directly in the poller's opaque user-data field. The event accessor returns the native field unchanged, including platform-specific non-Resource values; later Resource operations—not the accessor—validate any returned Handle.
_Avoid_: Completion queue, worker wakeup

**Thread-Pool Completion Source**:
The host-side notify handle corresponding to `thread_pool.c`'s `pool.notify_send`. Worker threads write or post completed job ids through it so `poll/wait` reports the completion source key, after which MoonBit drains `thread_pool/fetch_completion`.
_Avoid_: Host Poller, Barrier, worker wakeup
