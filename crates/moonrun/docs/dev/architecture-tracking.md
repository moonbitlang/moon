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
- [x] Make the regular Host Process path prepare the operation before execution,
  with clear ownership of interpretation, authorization, and admission.
- [x] Preserve native execution lookup wherever the primitive expresses the
  Runtime state: fully Ambient on every platform, and Ambient Env with Fixed
  cwd on Linux. Interpret Fixed cwd on Darwin and Windows instead of leaking
  host caller-directory behavior. Separate pure lookup calculation from OS
  observations and Host Process orchestration; interpreted Linux PATH search
  uses one child.

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

This completes the ownership and execution milestone. Host Process now prepares
native and virtual requests through one entry: existing lexical authorization,
Runtime state interpretation and child configuration, then a single-use
execution payload. Native lookup remains with libc or CreateProcessW when
both Env and cwd are Ambient. Linux can also express Fixed cwd with child
chdir. Darwin and Windows need explicit lookup for Fixed cwd because their
native primitives still observe the host caller directory. Policy presence
does not select this path. Owned Env supplies caller lookup inputs, including
absent PATH; child Env is separate. Virtual Moon derivation requires a plan
because Engine executes it instead of a native OS spawn. Interpreted lookup
captures cwd only when relative candidates need a base or Engine needs a
complete child configuration. Absolute lookup may inherit Ambient cwd without
requiring a recoverable pathname. The architecture guide records the native
reference, deliberate virtualization choices, and downstream async contract.

`process/spawn.rs` owns interpretation, authorization, and binding;
`process/lookup.rs` is pure candidate calculation with explicit inputs;
`process/ambient/lookup.rs` supplies OS observations. The split remains inside
the crate. Linux executes a prepared PATH search with one child and an execve
loop, while native Ambient search stays with libc. Permission coverage has not
expanded.
The remaining native execution and helper cancellation work can build on these
boundaries without moving Runtime interpretation into ABI adapters or workers.

## Session shutdown

- [x] Close managed and Virtual admission atomically with requesting termination
  of every admitted Engine member, including adopted descendants. Keep
  single-member termination separate and leave result reaping to the owner.
- [x] Let the virtual CLI finish cleanup after an unhandled signal, including
  signals received before root admission or while waiting for adopted children.
  Preserve ordinary non-virtual CLI signal behavior and normal orphan completion.

`Engine::request_shutdown` is irreversible for its process namespace and shared
by clones. It requests termination; `wait` still observes actual completion.
Native caller-thread Runs have no table membership and retain their independent
termination channels. The CLI records the first unhandled session signal,
requests shutdown, drains adopted roots, then presents that signal as its exit.
Repeated signals during cleanup do not bypass teardown. After finalization,
signal handling returns to the existing process fallback.

## Parallel authority work

Authority closure proceeds independently of command selection, preparation
helper cancellation, and native/Node execution. The
[workstream plan](design/authority-closure.md) records ownership and acceptance
criteria. WASI filesystem effects now use Host Filesystem authorization and
native execution, in addition to WASI rights and path containment. Child
restriction composition and explicit resource delegation remain follow-ups.

Host-global embedding ownership is separate: Job/process-group lifetime belongs
with helper and native execution; signal, Winsock, and diagnostic ownership can
be addressed independently. Authority changes include only embedding fixes
necessary for their enforcement. Execution strategies must preserve authority
without duplicating policy inside argument parsing or OS primitives.

## Follow-up order

1. Manage preparation helpers and their compiler descendants as owned work;
   cancel them without admitting the delivered program afterwards. This remains
   a FIXME: shutdown currently retains cancellation and waits for helper exit.
   Prefer a Unix process group / Windows Job for ordinary noninteractive build
   work. First narrow virtual command selection and separate it from the
   artifact-preparation protocol. Virtual spawn currently checks only the
   `moon`/`moonx` executable name and unconditionally adds the handoff prefix;
   Moon accepts that prefix only for `moon run` and `moonx`. Consequently,
   `moon build` is rejected while `moon run -` enters preparation. Ordinary
   Native spawn does not use this dispatch. The intended scope is build
   preparation and moonx, not general interactive Moon commands; public
   `moon build` must retain build-only semantics. A dedicated preparation
   entry (for example under `moon tool`) should reuse Moon's build logic and
   moonx's existing artifact delivery without inheriting all of `moon run`'s
   input modes. Reuse Moon's authoritative argument parser, including global
   options and opaque argument tails, rather than duplicating its grammar in
   moonrun. If Host Process must classify before spawning, extract the pure
   parser for both consumers; do not add a separate probe process. Parsing and
   execution-strategy selection remain distinct responsibilities. Deliver this
   command-selection work separately from both session shutdown and helper
   cancellation.
   Do not expand this step into a Moon/n2 cooperative cancellation protocol by
   default; n2 waits for running tasks but does not actively cancel them.
2. Execute native programs and Node through shared supervision. Define ambient
   versus constrained native authority and preserve artifact dependencies and
   execution location.
   Add external Moon/moonx delegation only with an explicit execution and
   authority contract behind Engine's interface.
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
- Native Ambient lookup preserves platform observation timing. Native-control
  regressions compare caller/child cwd behavior on each platform, including
  relative PATH entries. Separate Runtime tests exercise Fixed caller cwd and
  an independent child-cwd override; native fidelity alone does not prove cwd
  virtualization. Selection depends on Runtime state and platform behavior,
  not policy presence.
- Interpreted preparation uses caller Runtime PATH independently of child Env,
  resolves cwd according to the platform, and preserves lexical policy denial
  before lookup or transfer-file effects. Execution does not reinterpret its
  plan using later Runtime values. Cwd is captured only when required by the
  plan; otherwise the native operation can inherit it. Pure lookup tests need no OS or
  filesystem fixtures; execution tests cover Linux candidate retries within
  one child process.

- Spawn preparation failures follow async's completion/errno/free sequence;
  result handles are read only after success. Native controls cover the Linux
  compatibility executor's raw argv/envp and error behavior, plus Windows
  empty and NUL-terminated native strings.
