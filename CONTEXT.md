# MoonBuild Async Wasm Runtime

This context names the concepts used to describe `moonrun` support for running `moonbitlang/async` on the wasm backend.

## Language

**Semantic Stub Boundary**:
The operation-level contract exposed by `moonbitlang/async` native stubs, independent of native pointer layout or runtime object representation.
_Avoid_: Raw C ABI boundary

**Mapped Parity**:
A compatibility goal where wasm host behavior is tracked against native async stub operations without requiring a literal translation of the C files.
_Avoid_: Rewrite, line-by-line port

**Host Handle**:
An integer resource identity that the wasm guest can store and pass back while the host owns the underlying resource.
_Avoid_: Raw fd, raw HANDLE, externref

**Guest Owner Struct**:
A wasm-side value that keeps MoonBit-owned data reachable while the host has a pending operation referring to its guest-memory range.
_Avoid_: Pinned guest pointer

**Guest String Path**:
An async path argument passed from wasm to `moonrun` as a borrowed MoonBit `String` pointer plus a length measured in UTF-16 code units.
The guest must not pre-encode these paths as UTF-8 `Bytes`; `moonrun` converts the UTF-16 units into `OsString`, using the host's native path representation.
_Avoid_: UTF-8 path bytes, C string path

**Source Provenance Import**:
A `moonbit_v0` import declared together with the async C-stub source file and native symbol it tracks.
_Avoid_: Untraceable host helper

**Ported Symbol Origin**:
A V8-free Rust host implementation entry that records the async C-stub file and native symbol it is ported from. Active mapped imports must have both registry provenance and implementation provenance.
_Avoid_: Source comments that tests cannot verify

**Async Sys Module**:
A V8-free, source-shaped Rust module that owns the behavior of a native async C-stub operation. It may use `AsyncHost` for shared runtime state, resource tables, guest-memory helpers, or shared ABI representation types, but the operation logic belongs in the sys module.
_Avoid_: Thin wrappers around behavior hidden in `AsyncHost`

**Current Guest Memory**:
The `WebAssembly.Memory` object exposed as `moonbit_v0.memory` by the JS glue after instance creation or imported-memory discovery.
Host calls reacquire the current backing store for each import and never retain borrowed guest slices.
_Avoid_: Cached raw wasm pointer

**Async Monotonic Time**:
The wasm host value returned for async `ms_since_epoch`. It has millisecond precision and is monotonic from an arbitrary process-local origin; callers may compare values by subtraction, but the absolute value is not meaningful.
_Avoid_: Wall-clock epoch

## Boundary Decisions

- `moonrun` keeps V8 as the first adapter, but async host state remains outside V8 types.
- `AsyncHost` owns shared runtime state, resource tables, guest-memory helpers, and shared ABI representation types. `async_sys` owns ported operation behavior.
- `moonbit_v0` imports strip the native `moonbitlang_async_` prefix and do not add an `async_` prefix.
- Native C stubs are the semantic reference. Rust code should stay structurally close to the source files, but it does not link against `moonbit.h` object layouts.
- Wasm async time uses a monotonic host clock from an unspecified origin. Native C stubs currently use platform wall-clock APIs, but async timer semantics only require elapsed millisecond differences.
- The async wasm host currently supports only Unix-family and Windows hosts. Other host families are compile-time unsupported.
- Variable-length data crosses the boundary through guest offsets and explicit lengths. Async jobs store host-owned buffers plus guest offsets, then copy into freshly reacquired guest memory during a later host call.
- Async path arguments are the exception to byte-buffer transport: they cross as Guest String Paths so Windows reaches `OsString`/wide OS calls without a guest UTF-8 encode followed by host UTF-16 re-encode.
- V8 memory growth can replace the observable memory backing store. The runtime must not lend guest pointers to OS APIs that need pinned buffers across `memory.grow`; use host-owned pinned buffers and copy to/from wasm memory instead.
- Windows APIs that require stable buffers should receive host-owned memory, not raw wasm memory. This includes overlapped IO and other APIs where the OS may retain a pointer until asynchronous completion.
