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
requirements for library embedding, one execution-host interface, the
three host-operation layers, per-Runtime virtualization, authority mapping, and
child derivation. Its implementation
guidance and acceptance scenarios apply to every item below. This file owns
sequencing and completion status, not a second copy of those principles.

## Current milestone

Only this milestone is expanded into implementation work. Preserve ordinary
Wasm invocation and its arguments, environment, cwd, stdio, policy, and exit
behavior. Virtual process interfaces have no current consumers: change them
directly when needed for the intended architecture, without compatibility
wrappers or deprecation stages. Complex executable discovery is deferred.

- [x] Separate artifact preparation from execution admission and reaping. Keep
  one termination receiver alive through preparation, Run execution, and
  cleanup; a Run borrows that control without finalizing its owner.
- [x] Replace scattered Runtime construction arguments with one configuration
  transferred through preparation to Runtime, retaining the public RunOptions
  builders and existing policy behavior.
- [x] Unify public execution and supervision in Engine. New Engines own separate
  namespaces; clones share backend and process control. Preserve caller-thread
  `run`, managed `spawn`/`wait`, reusable Modules, and shared `run_moon` /
  `spawn_moon` preparation without a separate command model.
- [x] Complete the host-creation and guest-derivation interfaces, keeping one
  authoritative source for each field and making the source of inherited state
  and authority explicit rather than deriving either from a parent PID. Make
  the host configuration available through `with_environment`, including an
  explicit empty environment. Select Native or Virtual guest process behavior
  independently of thread placement. Only Virtual host entries capture missing
  root inputs; guest derivation supplies complete Runtime inputs.
- [ ] Make the regular Host Process path prepare the operation before execution,
  with clear ownership of interpretation, authorization, and admission.

Each independently reviewable change should test its intended behavior through
its execution interface. Existing virtual interfaces and tests do not impose a
backward compatibility requirement. Architectural vocabulary
belongs in the crate glossary; implemented behavior belongs in the developer
index and its linked references. Keep this file focused on sequencing and
remaining work.

Engine now owns execution scopes, including file loading and Moon artifact
preparation. `prepare_artifact` borrows control and produces an artifact;
Engine executes it and finalizes the result after cleanup. `RuntimeConfig`
remains the single set of construction inputs. The process table stores only
relationships, control, and completion, so it does not retain Runtime through an
Engine ownership cycle. The public tree wrapper and internal command module
have been removed.

This completes the library-interface and configuration slice. The general
Host Process path still separates policy checks from final operation
configuration and executable resolution. Its remaining work may revise the
experimental interfaces to clarify ownership; permission coverage has not
expanded with this refactor.

## Follow-up order

1. Finish Host Process preparation so interpretation, authorization, and
   admission use one resolved operation. Preserve current policy coverage.
2. Manage preparation helpers and their compiler descendants as owned work;
   cancel them without admitting the delivered program afterwards. Give the CLI
   a session-level shutdown contract, including adopted descendants.
3. Execute native programs and Node through shared supervision. Define ambient
   versus constrained native authority and preserve artifact dependencies and
   execution location.
   Add external Moon/moonx delegation only with an explicit execution and
   authority contract behind Engine's interface.
4. Extend total authorization and monotonic child restrictions, including WASI
   grants and explicit resource delegation.
5. Complete executable lookup compatibility, Windows special paths, trusted
   Moon identification, and stable file identity as required by the isolation
   contract.
6. Add cancellation to synchronous host operations individually. Completion
   continues to require actual teardown.

Concrete isolation problems on normal paths may be fixed independently. In
particular, audit the embedding ownership of process-global facilities such as
Winsock cleanup, Unix signal handlers, and the Windows global Job Object, plus
diagnostic routing. An owned Env alone does not isolate native library state.
These follow-ups do not all belong in the current milestone.

## Verification targets

- Termination remains authoritative through file loading, preparation, Run
  execution, and cleanup. Managed completion waits for thread reaping.
- Cancelling a wait preserves the child and its unread result; terminating one
  member leaves other members usable.
- Root and recursive guest execution preserve arguments, cwd, environment,
  stdio, inherited policy, and executable identity through artifact handoff.
- Ordinary Engine execution retains its standalone termination and teardown
  contract across supported backends.
- `run` and `spawn` preserve the selected guest process mode; Engine clones
  share control while new Engines have independent namespaces. Explicit empty
  environments do not fall back to host values.
