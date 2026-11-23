---
status: proposed
---

# Separate Command, Build, and Runtime Responsibilities

Moon's commands share dependency management, build, and execution capabilities.
Keeping those capabilities inside command implementations makes reuse depend on
CLI behavior; treating every shared implementation as a utility hides which
layer owns its rules. We separate command policy, build services, and runtime
execution, and expose reusable capabilities through library interfaces.

## Responsibilities

| Scope | Responsibility |
| --- | --- |
| `moon` | Command interpretation, command-specific policy and selection, invocation of library operations, and presentation of command results. |
| `mooncake` | Module Resolution, registry interaction, dependency acquisition, and package-management operations. |
| `moonbuild` | Project and script preparation, coordination of the build lifecycle, Execution Plan execution, and build diagnostics and publication. |
| `moonbuild-rupes-recta` (RR) | Package discovery and Package Resolution, Build Plan generation, and lowering to Execution Plans. |
| `moonrun` | Execution of Wasm programs and ownership of their runtime state, capabilities, and lifecycle. |
| `moonx` | The workflow that obtains or builds a program and arranges its execution by composing dependency, build, and runtime capabilities. |
| `moonutil` | Definitions and foundational operations shared across these layers, with explicit semantic ownership and no dependency on higher-level workflows. |

The `moonx` workflow includes acquisition and building, while reusing the
implementations owned by the build libraries. Its responsibility is independent
of executable packaging; [ADR 0003](0003-dispatch-moonx-by-executable-name.md)
records the current executable dispatch decision.

## Dependency Direction

Command and workflow code calls library operations. `moonbuild` coordinates
`mooncake` and RR; those libraries remain usable without depending back on
`moonbuild` or `moon`. Shared definitions must remain usable without importing
the workflows that consume them. Code shared by several callers retains its
domain owner rather than automatically moving into `moonutil`.

Program execution consumes artifacts and execution requests. When a running
program needs an artifact to be acquired or built, that work is delegated to
the build capabilities through an explicit request. The runtime owns execution
semantics; the build libraries own dependency and compilation semantics.

Each layer resolves the decisions it owns once the required inputs are
available. Later layers consume those decisions rather than reconstructing
command intent or repeating another layer's rules. This establishes semantic
ownership without requiring every external observation to happen eagerly or
claiming that an entire crate is free of I/O.

## Rationale and Consequences

Separating planning from coordination and execution lets the same build rules
serve different commands and library callers. Separating Module Resolution from
Package Resolution keeps registry and source acquisition concerns independent
of package relationships and build planning. Separating artifact preparation
from runtime execution lets both use their own lifecycle and resource model.

Library interfaces should express useful operations through explicit inputs and
results. CLI parsing and presentation remain adapters to those capabilities.
Public interfaces must support real consumers; implementation details and
intermediate state should stay private. Prefer consolidating related behavior
and reducing what callers must know over adding forwarding layers or duplicate
models.

These responsibilities guide module placement without prescribing individual
types or files. The [source map](../dev/contributing.md#source-map) records current
entry points. A further crate split needs a concrete benefit from an enforced
dependency boundary, independent use, or independent validation; responsibility
separation can first be established within existing crates.
