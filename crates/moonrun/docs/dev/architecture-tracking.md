# Moonrun architecture tracking

This file records implementation status and work dependencies following
[#2401](https://github.com/moonbitlang/moon/pull/2401). Contracts and acceptance
criteria belong in the [runtime architecture guide](design/runtime-architecture.md)
and [authority design](design/authority-closure.md), under
[ADR 0012](../adr/0012-preserve-embeddability-and-runtime-owned-host-semantics.md).

## Ownership and execution

- [x] Separate artifact preparation from execution admission and reaping.
- [x] Carry Runtime construction inputs through one configuration.
- [x] Unify execution and supervision in Engine.
- [x] Separate Engine membership and reported guest identity from child control.
- [x] Resolve command recognition before execution-specific resource binding.
- [x] Separate host creation from guest child derivation, with explicit state
  and authority inputs.
- [x] Prepare Host Process requests before execution, with single-use inputs.
- [x] Separate native process mechanisms from worker cancellation, policy
  transfer, Resource publication, and guest status encoding.
- [x] Separate pure executable lookup from OS observations and preserve native
  lookup where the platform can express Runtime state.

Contracts: [execution ownership](design/runtime-architecture.md#library-contract-and-execution-ownership),
[host-operation layers](design/runtime-architecture.md#three-layers-for-host-operations),
and [virtualization](design/runtime-architecture.md#virtualization-and-inheritance).
Evidence: [execution acceptance cases](design/runtime-architecture.md#applying-the-design-to-changes).

## Session shutdown

- [x] Close managed and Virtual admission while requesting termination of all
  admitted Engine members.
- [x] Forward CLI signals through namespace shutdown and adopted-root reaping.

Contract and limits: [execution ownership](design/runtime-architecture.md#library-contract-and-execution-ownership).

## Moonx-only guest execution

- [x] Share moonx's authoritative grammar between Moon and Moonrun, including
  its opaque program-argument tail. Keep ordinary Moon CLI definitions in Moon.
- [x] Restrict guest Virtual spawn to moonx for local `.mbtx` files and registry
  packages; reject other commands without native fallback.
- [x] Complete help/version and argument errors through the embedded frontend
  and bound streams before creating a helper or handoff directory.

Contract: [command selection](design/runtime-architecture.md#artifact-preparation-and-command-selection).
Evidence: [frontend tests](../../tests/moonx_frontend.rs) and
[recursive guest tests](../../tests/recursive_moon.rs), using local scripts and
private cached registry artifacts.

## Execution work

Deliver the remaining items as separate changes:

- [x] Select virtualization through `process.virtualize` in the policy file,
  with explicit embedding and `moonrun --virtual-process` overrides. IPC uses
  `process.ipc`, `with_ipc`, or `moonrun`/`moonx --daemon`. Keep existing `spawn`/`allow` permissions
  in both modes. Load selected configuration
  once while preserving inherited restriction composition.
- [x] Share moonrun's CLI grammar between the binary and library frontend
  adapters, keeping parsing separate from execution and process-global setup.
- [ ] Add native process members to Engine for ordinary authorized guest
  commands, preserving env, cwd, stdio, termination, wait, shutdown, and reaping.
- [x] Limit artifact handoff to Wasm; native/Node commands use ordinary OS spawn
  rather than a separate delivered-artifact executor.
- [ ] Extend internal classification from moonx to moonrun using shared CLI
  grammar. Moonrun loads Wasm directly; it does not need a preparation helper.
  Both frontends use existing process authorization and Engine execution.
- [ ] Reuse native process management for preparation helpers and compiler
  descendants. The preparation FIXME waits for helper exit before publishing
  cancellation. Use a Unix process group or Windows Job for noninteractive
  preparation; Moon/n2 cooperative cancellation is a separate protocol.

Contracts: [artifact preparation](design/runtime-architecture.md#artifact-preparation-and-command-selection)
and [execution ownership](design/runtime-architecture.md#library-contract-and-execution-ownership).

## Authority work

- [x] Apply Host Filesystem policy across unstable, async, and supported WASI
  filesystem operations, retaining WASI rights and preopen containment.
- [x] Separate policy source loading, resolved restrictions, Env provisioning,
  and external transfer; use typed inheritance for virtual children.
- [x] Compose inherited restrictions with additional child restrictions.
  Preserve the conjunction through descendant policy transport.
- [ ] Define explicit resource delegation and attenuation for files, sockets,
  stdio, and directory resources.
- [ ] Audit indirect host effects, including SQLite auxiliary files, TLS trust
  discovery, clocks, randomness, and library initialization.
- [ ] Define helper and native-execution grants together with execution work.

Contracts: [authority design](design/authority-closure.md). Acceptance cases:
[filesystem checks](design/authority-closure.md#filesystem-acceptance-criteria),
[policy composition](design/authority-closure.md#policy-composition),
and [resource delegation](design/authority-closure.md#resource-delegation).

## Embedding and compatibility work

These tasks have no dependency on command classification. Concrete isolation
failures on ordinary paths can be addressed as separate fixes.

- [ ] Coordinate process-global signal dispositions, Winsock lifetime, and the
  Windows Job containing the embedding process.
- [ ] Route diagnostics through an embedding-owned destination; policy denial
  writes to host stderr.
- [ ] Separate OS mechanisms in Job runners and `async_sys` from ABI and domain
  responsibilities as those operations change.
- [ ] Complete executable lookup compatibility, Windows special-path handling,
  trusted Moon identification, and stable file identity.
- [ ] Add cancellation to synchronous host operations that can block teardown.

Contracts: [embedding](design/runtime-architecture.md#library-contract-and-execution-ownership),
[lookup](design/runtime-architecture.md#virtualization-and-inheritance), and
[ownership across host facilities](design/authority-closure.md#ownership).
