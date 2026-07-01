# Async Wasm Host Boundary

We will support `moonbitlang/async` on the wasm backend through `#cfg(target="wasm")` bindings to a `moonbit_v0` host module in `moonrun`, without compiler changes or JS backend changes. The host follows the semantic contract of async's native C stubs, but wasm resources are represented as Handles and guest-memory ranges rather than native MoonBit runtime objects or pinned pointers; this keeps V8 as the first adapter while allowing the V8-free host core to be reused by future wasm runtimes.

## Decisions

- Use a semantic C-stub boundary: `moonbit_v0` symbols map from `moonbitlang_async_*` symbols by stripping that prefix.
- Keep source provenance next to import registration and V8-free host implementation. Each mapped import declares the async source file and native symbol it tracks, each active Rust port declares the same origin through the port provenance macro, and tests verify the registry and implementation entries stay in sync. V8 callback modules are adapters only and should not own port provenance.
- Keep `AsyncHost` focused on shared runtime state, the Handle table, guest-memory helpers, and shared ABI representation types. Ported operation behavior belongs in `async_sys`, with `AsyncHost` passed in only when the operation needs shared state.
- Use monotonic elapsed milliseconds with an unspecified process-local origin for wasm async time. The async timer contract uses differences between readings; the absolute epoch is not meaningful.
- Support Unix-family and Windows hosts first. Other host families are deliberately compile-time unsupported until the async C-stub parity target is defined for them.
- Store wasm memory as `moonbit_v0.memory` in the JS glue. The Rust adapter reads that property on each memory-using import instead of registering a separate async `set_memory` callback.
- Never retain raw wasm-memory pointers after an import returns. Host state may store handles, guest offsets, lengths, and host-owned buffers.
- Pass async path arguments as borrowed MoonBit `String` pointers plus UTF-16 code-unit lengths. The guest must not encode `OsString` paths to UTF-8 `Bytes` before calling `moonbit_v0`; `moonrun` owns conversion from MoonBit string data into Rust `OsString` and then into the native OS call form.
- Treat V8 memory growth as a reason to reacquire memory every call. OS APIs that need stable pointers must use host-owned memory and copy to or from wasm memory.
- Keep worker threads out of wasm guest memory. Worker jobs may compute host-owned results, but guest-memory writes happen only during guest-thread imports such as `fetch_completion` or payload-specific result accessors, where the V8 adapter can reacquire the current memory view.
- Treat wasm `FileTime` as a portable 48-byte record, not as a native `stat` or `FILE_BASIC_INFO` buffer. The wasm layout is little-endian `{ atime_sec: i64, atime_nsec: i32, mtime_sec: i64, mtime_nsec: i32, ctime_sec: i64, ctime_nsec: i32 }` with 4 bytes of padding after each nanosecond field, matching the WIT canonical record layout for those fields on wasm32.
- Make `poll` the wasm event-loop ABI. MoonBit owns scheduling and dispatch; moonrun owns the OS poller and returns readiness/completion events through `poll/create`, `poll/wait`, `poll/event_fd`, and related imports. `runtime/wait_for_event` is not part of the async wasm boundary.
- Register only the imports produced by the current wasm async implementation. Do not add deferred C-host symbols as placeholder unsupported imports; add future sockets, TLS, direct I/O, or file-watching surfaces only when wasm bindings import them and the implementation can be wired end to end.

## Module Boundaries

- `async_api` is the V8-facing `moonbit_v0` adapter. It declares and registers imports, decodes wasm ABI values from V8 callback arguments, reacquires guest memory, sets return values, and turns host failures into return values or traps.
- `async_host` is moonrun-owned runtime state for one V8 `moonbit_v0` host instance. It owns the Handle table, host workers, completion queues, guest-memory helper types, and opaque poll instances. It intentionally does not mirror `moonbitlang/async`.
- `async_sys` is the V8-free native-stub port layer. Its implemented files follow the `moonbitlang/async` source layout where the wasm backend imports that behavior, and each ported operation records the native source path and symbol it tracks. The platform poll files are direct ports behind the wasm `poll/*` imports.

## Event Loop Scope

Native `moonbitlang/async` has a platform poller: epoll on Linux, kqueue on macOS/BSD, and IOCP on Windows. Thread-pool completions are only one event source in that poller: Unix workers write completed job ids to a notify pipe registered with the poller, and Windows workers post completed job ids to the IOCP.

The wasm host exposes the same event-loop concept through opaque poll handles. `poll/register` and `poll/remove` operate on moonrun file-like Resource Handles, so pipes and future pollable handles stay opaque to the guest while Rust resolves them to the platform fd or HANDLE. The thread-pool completion source is created inside a poll instance, returned as an fd-like event key, and drained through `thread_pool/fetch_completion` when `poll/event_fd` reports that key.

Current filesystem reads and writes still run as worker jobs; socket, HTTP, and process work should extend the same poll facade instead of adding another wait shortcut. A Rust `Barrier` or completion-only `Condvar` is not enough because neither can wait on thousands of OS readiness events.

## Deferred

Executor design, cancellation semantics, broad core FS, sockets, process readiness, TLS, file watching, arbitrary external fd/HANDLE adoption, Wasmtime/WasmEdge adapters, and wasm-gc support remain follow-up work.
