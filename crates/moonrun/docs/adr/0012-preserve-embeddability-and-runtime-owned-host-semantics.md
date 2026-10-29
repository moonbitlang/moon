---
status: accepted
---

# Preserve Embeddability and Runtime-Owned Host Semantics

Moonrun is an execution library with a CLI adapter. Embedding callers must be
able to reuse an Engine and Loaded Modules, choose execution placement, and
observe guest outcomes without surrendering the host process's lifecycle.
Supervision is optional: a Supervisor owns commands and their control, while a
Run executes Wasm and a Runtime owns the virtual environment used by that Run.

Host behavior follows three semantic layers: ABI adaptation, Host Domains, and
Raw Operations. Each Host Domain interprets and authorizes an operation against
canonical Runtime State before invoking platform or library mechanisms. Runtime
composes domains; it does not forward every operation. This keeps backend and
ABI changes from duplicating environment, path, or permission rules. It also
avoids a universal OS wrapper or orchestrator that merely repeats those rules.

Virtualization and authorization are separate obligations. Owned state prevents
accidental sharing; authorization limits reachable effects. Effective Authority
must account for domain policy, WASI, delegated resources, and any helper or
native execution. Guest derivation must not widen delegated authority. A virtual
PID, an owned environment, or a Moonrun Policy file alone is not a complete
sandbox guarantee. Ambient compatibility remains explicit while these contracts
are implemented incrementally.

The [architecture guide](../dev/design/runtime-architecture.md) specifies these
constraints, their code owners, current gaps, and review obligations. The
[tracker](../dev/architecture-tracking.md) owns implementation order rather than
redefining the design.
