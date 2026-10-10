# Moonrun Runtime

Shared MoonBit host implementations and V8/Wasmtime adapters. The `moonrun`
CLI depends on this crate and retains its ambient process behavior.

Runtime vocabulary and host ABI contracts are maintained in
[`moonrun/CONTEXT.md`](../moonrun/CONTEXT.md) and its
[developer documentation](../moonrun/docs/dev/README.md).

Select Wasmtime with `default-features = false, features = ["wasmtime"]`.

An embedder loads a Module once, shares its Engine, and uses
`Engine::run_in_context` with a fresh `ExecutionContext` per request. The context
owns standard streams and resolves relative filesystem paths and native child
cwd without changing the process directory. Guest arguments and capability
policy remain in `RunOptions`; a context alone does not provide a sandbox.

`ChildLauncher` lets Unix callers validate or replace an authorized native
command and supply inherited lifetime files. `RunControl` cancels the run and
supervises its Unix native process groups. Wasmtime CPU cancellation requires
`EngineConfig::with_epoch_interruption(true)` and caller-driven
`Engine::increment_epoch()` ticks. Each run still executes synchronously on its
calling thread. This is cancellation, not async scheduling or fuel yielding.
V8 retains its existing cooperative signal handling; epoch interruption is
specific to Wasmtime.

The library does not install the CLI signal broker. Unrestricted guest
environment access still observes and mutates the process environment, so
concurrent embedders must supply an explicit environment policy. Native
sandbox policy, request threads, temporary directories, logging and cleanup
belong to the caller.
