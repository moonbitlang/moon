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
should converge on one execution-host entry point that integrates Engine and
Supervisor responsibilities internally. A caller must not need to compose those
owners or understand a Command object to execute a program. Internally, Engine
prepares and executes Wasm, Supervisor owns admission and lifecycle, and Runtime
owns the virtual environment. The command module composes preparation, execution,
and cleanup rather than defining another public object model.

Preparing a Moon/moonx artifact for execution in the embedding process or
delegating the invocation to an external process is an execution strategy
behind that interface. This choice
does not grant additional authority or change who owns completion and cleanup.
Backend selection is separate from this strategy. Synchronous Run execution
must remain available on the
calling thread. Exact public types and method names remain to be designed.

This public-interface convergence is follow-up work. The implementation in
[#2403](https://github.com/moonbitlang/moon/pull/2403) retains the existing public
`Engine` and `VirtualProcessTree` interfaces while establishing their internal
ownership; it does not yet provide the unified entry point.

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

The additional decisions are the internal separation of Supervisor, command
execution, Run, and Runtime ownership behind a converged execution-host
interface; the host-creation versus guest-derivation contract; and an authority
ceiling spanning all exposed surfaces. The three-layer model
consolidates the existing domain separation and replaces the repository ADR's
early V8-specific module allocation. It does not replace the native-shaped
Worker/Job contract, acquired Resource ownership, or the established ABI and
Guest Memory decisions.

The [architecture guide](../dev/design/runtime-architecture.md) specifies these
constraints, their code owners, current gaps, review obligations, and
[relationship to earlier decisions](../dev/design/runtime-architecture.md#relationship-to-earlier-decisions).
The [tracker](../dev/architecture-tracking.md) owns implementation order rather
than redefining the design.
