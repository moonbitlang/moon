# Moonrun Runtime

Shared MoonBit host implementations and V8/Wasmtime adapters. The `moonrun`
CLI depends on this crate and retains its ambient process behavior.

Runtime vocabulary and host ABI contracts are maintained in
[`moonrun/CONTEXT.md`](../moonrun/CONTEXT.md) and its
[developer documentation](../moonrun/docs/dev/README.md).

Select Wasmtime with `default-features = false, features = ["wasmtime"]`.
