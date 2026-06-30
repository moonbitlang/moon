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
The wasm linear memory owned by the guest program.
_Avoid_: Wasm buffer, V8 memory

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

**Resource**:
A moonrun-owned OS or runtime resource exposed to MoonBit through an opaque async boundary value.
_Avoid_: Capability, Host Resource, Guest Resource, raw fd, pointer, id

**Resource Handle**:
An opaque value held by MoonBit code that names a resource at the async boundary.
_Avoid_: Host Handle, Guest Handle, raw fd, pointer, id

**Native-Shaped Async Boundary**:
The wasm async host boundary that keeps MoonBit-facing concepts aligned with `moonbitlang/async` native concepts even when moonrun uses different host representations.
_Avoid_: Wasm-specific async API, shortcut API

**Native Behavior**:
The observable behavior of `moonbitlang/async` native execution that moonrun should match byte-for-byte unless that behavior is questionable or not user facing.
For normal MoonBit async paths, moonrun should stay strictly native-shaped and avoid adding observable intermediate states. Extra validation exists at the async boundary to reject stale or unexpected calls from an Untrusted Guest before they can violate moonrun's Rust or OS ownership invariants.
_Avoid_: Conceptual parity, best-effort compatibility

**Async API**:
The V8-facing `moonbitlang/async` adapter that registers imports, decodes wasm ABI values, reacquires guest memory, sets return values, and reports traps.
_Avoid_: Runtime state, native-stub implementation

**Async Host**:
Moonrun-owned async runtime state for one V8 `moonbitlang/async` host instance: handle tables, host workers, completion queues, guest-memory helper types, and opaque host poll instances.
_Avoid_: `moonbitlang/async` source mirror

**Async Sys**:
The V8-free native-stub port layer. Implemented files follow the `moonbitlang/async` source layout and carry provenance for the native source path and symbol they track. Poller files are direct ports behind the wasm `poll/*` imports.
_Avoid_: V8 adapter, placeholder unsupported imports

**Host Poller**:
The `async_sys::internal::event_loop::poll` port of native epoll, kqueue, or IOCP. The wasm event loop owns opaque `Instance` handles and calls `poll/wait`, `poll/event_fd`, and `poll/event_events`; moonrun resolves registered host fd handles to platform fds or HANDLEs.
_Avoid_: Completion queue, worker wakeup

**Thread-Pool Completion Source**:
The host-side notify handle corresponding to `thread_pool.c`'s `pool.notify_send`. Worker threads write or post completed job ids through it so `poll/wait` reports the completion source key, after which MoonBit drains `thread_pool/fetch_completion`.
_Avoid_: Host Poller, Barrier, worker wakeup
