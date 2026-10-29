---
status: accepted
---

# Preserve Embeddability and Runtime-Owned Host Semantics

This decision extends the repository's
[Async Wasm Host Boundary ADR](../../../../docs/adr/0001-async-wasm-host-boundary.md).
Backend-independent host behavior and Run-scoped termination were already
required there. Resource acquisition, Runtime-owned Handles, and protection of
embedding stdio are established by Moonrun ADRs
[0008](0008-own-resources-behind-boundary-handles.md),
[0009](0009-use-one-handle-namespace.md), and
[0010](0010-preserve-stdio-blocking-mode-for-compat-imports.md).
The Host Domain / Raw Operation vocabulary and the library interface also
predate this decision. These commitments remain in force.

Moonrun is an execution library with a CLI adapter. Embedding callers must be
able to reuse prepared modules, choose execution placement, and observe guest
outcomes without surrendering the host process's lifecycle. The public interface
uses `Engine` as its execution-host entry point. Engine prepares and executes
Wasm and owns process admission and lifecycle; Runtime owns the virtual
environment. A caller does not compose separate execution and supervision
objects. Preparation, execution, and cleanup form one lifecycle without an
additional command object or domain model.

Preparing a Moon/moonx artifact for execution in the embedding process or
delegating the invocation to an external process is an execution strategy
behind that interface. This choice does not grant additional authority or
change who owns completion and cleanup.
Backend selection is separate from this strategy. `Engine::run` executes on
the calling thread; `Engine::spawn` creates managed work whose result is consumed
by `Engine::wait`. `run_moon` and `spawn_moon` share preparation and artifact
execution, with an external helper preparing the artifact in both cases.
External delegation of the whole invocation remains future work.

Each new Engine owns an independent process namespace. Clones share its backend
and process table, while each Run realizes its own Runtime. The process table
retains control and completion rather than Runtime or Engine references, so a
Runtime's reference to its execution host does not create an ownership cycle.
Guest Process Mode is selected independently of placement: Native preserves
compatibility, while Virtual selects virtual child control and owned root
state. Public owned-environment configuration is available in either mode.

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

The additional decisions are the separation of Engine lifecycle, Run, and
Runtime ownership behind one execution-host interface; the host-creation versus
guest-derivation contract; and an authority ceiling spanning all exposed
surfaces. The three-layer model
consolidates the existing domain separation and replaces the repository ADR's
early V8-specific module allocation. It does not replace the native-shaped
Worker/Job contract, acquired Resource ownership, or the established ABI and
Guest Memory decisions.

The [architecture guide](../dev/design/runtime-architecture.md) specifies these
constraints, their code owners, current gaps, review obligations, and
[relationship to earlier decisions](../dev/design/runtime-architecture.md#relationship-to-earlier-decisions).
The [tracker](../dev/architecture-tracking.md) owns implementation order rather
than redefining the design.
