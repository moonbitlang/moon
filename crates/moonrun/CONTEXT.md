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

**Guest String Path**:
A MoonBit `String` pointer plus UTF-16 code-unit length used for async path arguments crossing `moonbit_v0`.
Moonrun converts this directly into `OsString`; guest code must not send UTF-8 `Bytes` for paths.
_Avoid_: Guest UTF-8 path buffer

**Host Buffer**:
Memory owned by moonrun while servicing guest jobs.
_Avoid_: Native buffer, temporary buffer

**Native-Shaped Async Boundary**:
The wasm async host boundary that keeps MoonBit-facing concepts aligned with `moonbitlang/async` native concepts even when moonrun uses different host representations.
_Avoid_: Wasm-specific async API, shortcut API
