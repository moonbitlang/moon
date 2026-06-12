# Async Wasm Host Boundary

We will support `moonbitlang/async` on the wasm backend through `#cfg(target="wasm")` bindings to a `moonbit_v0` host module in `moonrun`, without compiler changes or JS backend changes. The host follows the semantic contract of async's native C stubs, but wasm resources are represented as host handles and guest-memory ranges rather than native MoonBit runtime objects or pinned pointers; this keeps V8 as the first adapter while allowing the V8-free host core to be reused by future wasm runtimes.

## Decisions

- Use a semantic C-stub boundary: `moonbit_v0` symbols map from `moonbitlang_async_*` symbols by stripping that prefix.
- Keep source provenance next to import registration and V8-free host implementation. Each mapped import declares the async source file and native symbol it tracks, each active Rust port declares the same origin through the port provenance macro, and tests verify the registry and implementation entries stay in sync. V8 callback modules are adapters only and should not own port provenance.
- Keep `AsyncHost` focused on shared runtime state, resource tables, guest-memory helpers, and shared ABI representation types. Ported operation behavior belongs in `async_sys`, with `AsyncHost` passed in only when the operation needs shared state.
- Use monotonic elapsed milliseconds with an unspecified process-local origin for wasm async time. The async timer contract uses differences between readings; the absolute epoch is not meaningful.
- Support Unix-family and Windows hosts first. Other host families are deliberately compile-time unsupported until the async C-stub parity target is defined for them.
- Store wasm memory as `moonbit_v0.memory` in the JS glue. The Rust adapter reads that property on each memory-using import instead of registering a separate async `set_memory` callback.
- Never retain raw wasm-memory pointers after an import returns. Host state may store handles, guest offsets, lengths, and host-owned buffers.
- Pass async path arguments as borrowed MoonBit `String` pointers plus UTF-16 code-unit lengths. The guest must not encode `OsString` paths to UTF-8 `Bytes` before calling `moonbit_v0`; `moonrun` owns conversion from MoonBit string data into Rust `OsString` and then into the native OS call form.
- Treat V8 memory growth as a reason to reacquire memory every call. OS APIs that need stable pointers must use host-owned memory and copy to or from wasm memory.
- Keep unsupported MVP symbols registered when they are part of the mapped boundary, but make them return native-style unsupported errors instead of causing missing-import instantiation failures.

## Deferred

Executor design, cancellation semantics, broad core FS, sockets, process, TLS, file watching, arbitrary external fd/HANDLE adoption, Wasmtime/WasmEdge adapters, and wasm-gc support remain follow-up work.
