# Moonrun architecture tracking

The first milestone following [#2401](https://github.com/moonbitlang/moon/pull/2401)
establishes ownership and execution interfaces for the existing Wasm virtual
process path. It is complete when helper cancellation and native execution can
be added without redesigning Runtime state, guest imports, and supervision
together.

## Design contract

[ADR 0012](../adr/0012-preserve-embeddability-and-runtime-owned-host-semantics.md)
records the accepted direction. The
[runtime architecture guide](design/runtime-architecture.md) owns the lasting
requirements for library embedding, the three host-operation layers, per-Runtime
virtualization, authority mapping, and child derivation. Its implementation
guidance and acceptance scenarios apply to every item below. This file owns
sequencing and completion status, not a second copy of those principles.

## Current milestone

Only this milestone is expanded into implementation work. Preserve normal
Wasm invocation, arguments, environment, cwd, stdio, policy inheritance, virtual
PID, wait, and exit behavior. Complex executable discovery is deferred.

- [x] Separate command preparation and artifact execution from supervisor
  admission and reaping. Keep one termination receiver alive through command
  cleanup; a Run borrows that control without finalizing its owner.
- [x] Replace scattered Runtime construction arguments with one configuration
  transferred through preparation to Runtime, retaining the public RunOptions
  builders and existing policy behavior.
- [ ] Complete the host-creation and guest-derivation interfaces, keeping one
  authoritative source for each field and making the source of inherited state
  and authority explicit rather than deriving either from a parent PID. Make
  the host configuration available to library callers and exercise concurrent
  independent environments without the CLI.
- [ ] Make the regular Host Process path prepare the operation before execution,
  with clear ownership of interpretation, authorization, and admission.

Each independently reviewable change should preserve existing public behavior
and include tests through its execution interface. Architectural vocabulary
belongs in the crate glossary; implemented behavior belongs in the developer
index and its linked references. Keep this file focused on sequencing and
remaining work.

The first implementation separates command execution and preparation in
`process_tree::command`, keeps receiver finalization in the supervisor, and
makes Runtime consume `RuntimeConfig`. It fixes termination being rejected
between Wasm Run teardown and command cleanup. Native execution and helper
cancellation remain follow-ups; the first two items do not complete the whole
milestone.

## Follow-up order

1. Manage preparation helpers and their compiler descendants as owned work;
   cancel them without admitting the delivered program afterwards. Give the CLI
   a session-level shutdown contract, including adopted descendants.
2. Execute native programs and Node through shared supervision. Define ambient
   versus constrained native authority and preserve artifact dependencies and
   execution location.
3. Extend total authorization and monotonic child restrictions, including WASI
   grants and explicit resource delegation.
4. Complete executable lookup compatibility, Windows special paths, trusted
   Moon identification, and stable file identity as required by the isolation
   contract.
5. Add cancellation to synchronous host operations individually. Completion
   continues to require actual teardown.

Concrete isolation problems on normal paths may be fixed independently. In
particular, audit the embedding ownership of process-global facilities such as
Winsock cleanup, Unix signal handlers, and the Windows global Job Object, plus
diagnostic routing. An owned Env alone does not isolate native library state.
These follow-ups do not all belong in the current milestone.

## Verification targets

- Command termination remains authoritative through preparation, Run execution,
  and command cleanup, while completion waits for reaping.
- Cancelling a wait preserves the child and its unread result; terminating one
  member leaves other members usable.
- Root and recursive guest execution preserve arguments, cwd, environment,
  stdio, inherited policy, and executable identity through artifact handoff.
- Ordinary Engine execution retains its standalone termination and teardown
  contract across supported backends.
