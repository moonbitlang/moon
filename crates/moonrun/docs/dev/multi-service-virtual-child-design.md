# Multi-Service Runs and Virtual Child Processes

## Status

Proposed.

This design covers moonrun-owned host imports and the shared per-Run
environment observed by WASI. The WASI descriptor/preopen Capability Surface
is explicitly out of scope for the first implementation. The end state
requires Moonrun Policy to describe and authorize the complete moonrun-owned
guest world, but the work is deliberately split into independently useful
increments.

The supporting research is recorded in
[Operating-System Support for Moonrun Virtualization](os-virtualization-research.md)
and
[Scheduler Control-Flow Models for Moonrun](scheduler-control-flow-research.md).

## Executive summary

The existing Engine work is the execution kernel, not a competing design. It
already supplies reusable compiled Modules and fresh state for synchronous
Runs. This proposal deepens the system around it in four places:

1. Make every process-scoped dependency used by moonrun-owned imports an
   explicit per-Run input: environment, working directory, standard streams,
   Policy, and Run control.
2. Add an Execution Supervisor above Engine to own concrete attempts, threads,
   retained outcomes, wait, termination, and cleanup.
3. Add a Deployment Controller above the supervisor to own declarative
   multi-service desired state, reconciliation, and restart/replacement policy.
4. Deepen the existing Host Process so it can route a spawn through the
   supervisor and maintain the guest-visible child identity, authority, and
   reap state.

Operating systems provide excellent primitives for real processes and useful
low-level wakeup primitives for virtual Runs. They do not provide per-thread
environments, working directories, PIDs, process tables, or safe hard
termination. Those semantics must remain inside moonrun when multiple Runs
share one native process.

The first useful release is not the virtual process table. It is isolated
per-Run environment, working directory, standard streams, and control. An
Execution Supervisor then turns that shared trunk into retained, controllable
attempts. Deployment reconciliation and virtual child mapping are separate
callers of the supervisor rather than two modes inside one Interface.

## Goals

The design supports two use cases.

### Multi-service deployment

An external Deployment specification declares several named workloads. A
Deployment Controller compares desired and observed state and asks an
Execution Supervisor to start or stop concrete Run attempts. Workloads have
independent environment, working directory, standard streams, Policy, and
lifecycle. Logical workload identity remains stable when a later policy
replaces an attempt.

This follows the Kubernetes controller direction and the Nomad
client/task-driver split. It does not follow Wrangler Service Bindings:
Wrangler injects a callable capability into a runtime-invoked worker, whereas
moonrun needs an external owner for workload lifecycle. Service discovery and
request transport are downstream of lifecycle and remain separate.

### Virtual Moonx child

When a guest requests an allowed `moonx` child, moonrun may resolve the target
to a Module and execute it as another Run in the same native process. To the
parent guest, the child still has process-shaped spawn, wait, exit, standard
stream, and termination behavior. Moonrun must never pass its virtual process
identifier or handle to an operating-system process API.

The first MVP handles an exact, explicitly configured Moonx route for a Wasm
target. Native-target fallback, shells, detached children, orphan adoption, and
arbitrary command emulation are not required.

## Non-goals for the first implementation

- Virtualizing WASI descriptors or preopens.
- Claiming a security boundary equivalent to a container or separate OS
  process.
- Transparent hard CPU or memory isolation between in-process Runs.
- Emulating every platform-specific process operation in the first Moonx MVP.
- Defining a distributed scheduler, cluster consensus, or remote node protocol.
- Running `setenv`, `unsetenv`, or `chdir` around a Run and restoring them
  afterward. That is racy in a concurrent embedding process.

## Research conclusions

The research tested four hypotheses. The evidence supports the following
answers.

| Question | Conclusion | Design consequence |
|---|---|---|
| Can the OS isolate a real child? | Yes. POSIX spawn/exec and Windows process creation accept child environment and standard streams; Windows also accepts a child current directory. Native PID/handle, wait, signaling, Job Objects, pidfds, and kqueue all operate on real processes. | Keep a native child Adapter and delegate native lifecycle to the OS. |
| Can the OS make an Engine Run look like a process? | No. Process IDs, process tables, environment, current directory, process signals, Job Objects, pidfds, and cgroups name real processes or process-wide state. | Virtual identity, inheritance, wait, exit, and termination live in Host Process and per-Run host Modules. |
| Can the OS help wake the parent when a virtual Run changes state? | Yes. Linux eventfd, Darwin user events or an equivalent wakeup descriptor, and Windows IO completion packets integrate synthetic completions with native polling. | Reuse the existing Completion Queue/Thread-Pool Completion Source instead of adding periodic polling. |
| Should Linux namespaces be the portable foundation? | No. They are Linux-only, capability-sensitive, mostly process/thread-group oriented, and unsafe as a semantic dependency after an embedding process has started worker threads. cgroup resource domains also primarily account processes. | Treat namespaces/cgroups as an optional worker-process isolation Adapter, not as in-process semantics or an MVP dependency. |

The orchestration comparison adds four control-flow conclusions:

| Reference | What to borrow | What not to borrow |
|---|---|---|
| Kubernetes | external desired/observed state, reconciliation, stable workload identity distinct from replaceable attempts, owner-dependent cleanup | API server, distributed consensus, container-only runtime assumptions, Linux-centric node implementation |
| k0s | single-binary packaging and supervision of runtime processes | no separate lifecycle model; it preserves Kubernetes scheduling and controller semantics |
| Nomad | node-local execution ownership, retained task status, distinct start/wait/stop/destroy operations, execution-driver capabilities | out-of-process plugin protocol and the assumption that all OS adapters provide equal isolation |
| Wrangler | only a possible future request/RPC transport Adapter | runtime-injected Service Binding as the owner of deployment or child lifecycle |

A single-process MVP has one deterministic local placement, so it does not yet
need a Scheduler Seam. Placement becomes a real Seam only when a second
execution pool or remote node exists. Desired-state reconciliation and
imperative child creation are already distinct and must not be merged.

Two qualifications matter:

- Linux `openat2` can strengthen host-backed path confinement for individual
  filesystem operations, but it does not give a Run a process-independent
  current directory. Other platforms need equivalent handle-relative Host
  Filesystem implementations rather than a process `chdir`.
- OS process termination is safe because the target owns a separate address
  space. Forcefully terminating an arbitrary thread in the embedding process
  is unsafe. Virtual child termination must use the existing Run termination
  and V8 interruption path, followed by normal Host teardown.

## Existing foundation

This proposal preserves the responsibilities already established by moonrun:

- **Engine** compiles immutable Modules and executes one Run synchronously on
  the calling thread. It does not schedule Runs or retain lifecycle state.
- **Module** is an immutable, reusable prepared representation.
- **Host** is the per-Run composition root and owns one Handle namespace.
- **Async Host** already keeps Resources, Jobs, Workers, pollers, and the
  Completion Queue per Run.
- **Host Process** already owns Process Job payloads, authorization, native
  child ownership, and process-handle provenance. It is the correct Module to
  deepen; a parallel universal `ProcessTree` would split the same invariants.
- **Run Termination** already records exit or signal termination without
  terminating the embedding process.
- The existing per-instance signal channel already provides a thread-safe Run
  control path and integrates with V8 interruption and host poll wakeup.

The implementation may have to integrate these changes from separate lines of
work before the slices below start. That integration changes sequencing, not
the design.

## Target model

### Engine and per-Run inputs

`Engine::run` remains synchronous:

```text
Engine::run(Module, RunOptions) -> RunOutcome
```

`RunOptions` becomes a fully realized description of one Run rather than a set
of paths that Engine must interpret. It carries, directly or through owned
handles:

```text
RunOptions
  arguments
  initial environment
  initial working directory
  standard input, output, and error
  immutable Moonrun Policy
  Run control receiver
```

Policy-file parsing, inheritance from the embedding process, and Deployment
configuration are outer Adapter responsibilities. This keeps Engine independent
of CLI files and makes concurrent tests deterministic.

The current command-line Adapter preserves legacy behavior by providing the
ambient environment as a read-only source, snapshotting the current directory,
and borrowing the process standard streams before calling Engine. Env is the
only Module allowed to read that environment source, and moonrun never mutates
it. The embedding process must keep it immutable while any Run backed by it is
active.

### Policy describes; host Modules own mutable state

Moonrun Policy is the source of truth for the moonrun-owned guest world and its
permissions. It is immutable once a Run starts. It does not become a mutable
container for runtime state.

At Run construction, Policy and explicit embedding inputs materialize separate
per-Run host Modules:

| Guest-visible surface | State owner | Policy responsibility |
|---|---|---|
| arguments | Run adapter | supply or constrain initial values |
| environment | Env | select inherited entries, set initial entries, and constrain mutation/visibility |
| working directory and paths | Host Filesystem | define initial guest cwd, mounts/path mappings, and permitted operations |
| stdin/stdout/stderr | Host Stdio | select inherited, piped, captured, null, or supervisor-owned streams |
| files, sockets, Jobs, Workers, pollers | existing Host domains | authorize creation/use and any quotas |
| DNS/connect/bind | Host Network/Resource state | authorize endpoints and later resolve declared service names |
| child processes | Host Process | authorize a route, child inheritance, native escape, wait, and termination |
| signals and exit | Run Termination and Run control | constrain which controls may be sent or observed |
| time and randomness | dedicated host adapters where exposed by moonrun-owned imports | choose ambient, fixed/seeded, or future quota-aware providers |

The split gives Policy complete coverage without turning it into a giant stateful
Module. A child receives a snapshot of inheritable parent state plus an
attenuated Policy. Child authority must never exceed parent authority.

### Control-flow ownership

The two use cases share concrete execution but not intent or lifetime policy:

| Flow | Request owner | Lifecycle owner | Guest-visible owner |
|---|---|---|---|
| declared workload | outer Deployment submitter | Deployment Controller owns desired state; Execution Supervisor owns each concrete attempt | none |
| virtual child | parent guest initiates one imperative spawn | Execution Supervisor owns the accepted attempt; restart is `Never` | Host Process owns child identity, authority, wait, and reap semantics |
| service request | caller guest initiates an ordinary request | caller owns request/cancellation; Deployment Controller continues to own target workload lifetime | Host Network or later discovery Adapter owns name resolution only |

Inputs flow downward before execution. Status and Completion flow upward. No
runtime-injected callable object owns deployment topology or child lifetime.

### Execution Supervisor

Execution Supervisor is a deep Module above Engine and below both callers. It
owns concrete execution attempts, not declarative workload topology:

```text
start(owner, execution_spec) -> execution_ref
observe(execution_ref) -> execution_status
wait(execution_ref) -> outcome
terminate(execution_ref, deadline, reason)
destroy(execution_ref)
```

It retains the outcome after exit so wait-after-exit is reliable. `terminate`
requests graceful control and may escalate according to Adapter capability;
`destroy` removes retained state only after callers no longer need it. Engine
remains synchronous. The Engine Run Adapter places `Engine::run` on a
supervisor-owned thread and reports the outcome upward.

One concrete attempt has a distinct identity and lifecycle:

```text
pending -> starting -> running -> stopping -> exited -> destroyed
                         └──────────────────> exited
```

Workload identity, child identity, attempt identity, thread identity, and OS
PID are different concepts. A service replacement creates a new attempt. The
first virtual-child MVP maps one child identity to exactly one attempt.

The execution Seam becomes public inside moonrun only when two Adapters are
present:

- Engine Run Adapter: synchronous Engine on a supervisor-owned thread, virtual
  control, retained Run outcome;
- Native Process Adapter: OS spawn, wait, signal/terminate, and native process
  resources.

The useful Interface semantics follow Nomad's task drivers—start, wait, stop,
and destroy remain distinct—without copying its plugin protocol.

### Deployment Controller

Deployment Controller owns declarative desired state and reconciliation:

```text
DeploymentSpec
  workloads: WorkloadName -> Module + Run template + lifecycle policy

Deployment Controller
  apply(spec) -> revision
  observe(deployment) -> desired/observed status
  delete(deployment) -> revision
```

On each reconciliation, the controller compares desired workload instances
with observed Execution Supervisor records and starts, stops, or later replaces
attempts. The first MVP has one local placement, one instance per workload,
and restart policy `Never`. A Scheduler Seam is deferred until there is a
second placement target such as another process, execution pool, or remote
node.

This separation follows Kubernetes controller versus kubelet ownership and
Nomad server versus client/driver ownership. The controller never calls Engine
directly and the supervisor never interprets Deployment topology.

### Service discovery

Discovery observes running workloads and publishes stable names to current
endpoints; it does not create or keep workloads alive. The caller Run retains
normal request control flow:

```text
caller guest -> Host Network/discovery -> endpoint -> target workload
```

The first multi-service MVP does not require discovery. A later discovery Seam
is justified when native endpoint routing and direct in-process request routing
both exist. Either Adapter must preserve the same request, cancellation, and
failure semantics without taking ownership of target lifecycle.

### Process routing Seam

Host Process owns one normalized spawn request. After authorization it asks
Execution Supervisor to start the selected execution kind:

```text
Spawn request
  -> Policy authorization and route selection
     -> Execution Supervisor
        -> Native Process Adapter -> OS process
        -> Engine Run Adapter     -> Engine Run
```

Routing is explicit and deny-by-default when Policy is active. An allowed
virtual route must not silently fall back to a native executable if resolution
fails. Native escape has a separate explicit rule because it grants ambient OS
authority that cannot be attenuated by moonrun's in-process host Modules.

The normalized route result distinguishes at least:

```text
NativeExecution(program, argv)
EngineExecution(target, argv, child_policy)
Deny(reason)
```

The initial rule matches only the intended Moonx executable spelling and Wasm
target form. Shell syntax, PATH-dependent aliases, and package-runner expansion
are excluded until their semantics are individually designed.

### Host Process child table

Host Process keeps a per-parent child table. Internally, a child is tagged; a
virtual child is never represented as a raw OS process:

```text
Child
  Native
    guest-visible native PID projection
    ExecutionRef
  Virtual
    guest-visible virtual child identifier
    ExecutionRef
```

The guest-visible ABI may still be PID-shaped. Host Process therefore owns a
mapping from each guest-visible child identifier and process Handle to the
tagged Child. Native children may preserve the OS PID for compatibility.
Virtual identifiers are allocated by Host Process, collision-checked against
all live entries, and resolved before any OS call. The exact encoding is an ABI
decision and must not leak into domain logic.

A virtual child has a parent-facing lifecycle distinct from the supervisor's
attempt lifecycle:

```text
starting -> running -> exited -> reaped
```

- `spawn` allocates and publishes the entry before the background Run can
  complete.
- Execution Supervisor asks Engine Run Adapter to start one
  supervisor-owned thread that invokes `Engine::run`.
- Moonx argument or target errors that would occur after a real Moonx process
  started are written to the child's stderr and become its exit result; they
  do not retroactively turn a successful spawn into an OS spawn error.
- Execution Supervisor atomically retains one exit result. Host Process
  translates its notification into the existing Completion Queue path so
  parent wait uses no polling.
- Wait and process-handle operations resolve the child table first. Native
  and virtual entries then delegate to Execution Supervisor, whose selected
  Adapter owns OS or Engine mechanics.
- Termination asks Execution Supervisor to stop the attempt. Engine Run
  Adapter sends through Run control. Completion is published only after the
  child Host has torn down, so the parent never observes an exited child whose
  Resources or Workers are still live.
- Reaping removes guest reachability according to existing platform-shaped
  semantics. Dropping the last handle never passes a virtual identifier to an
  OS close or wait function.

Detached virtual children and orphan adoption are deferred. In the MVP, parent
teardown asks Execution Supervisor to terminate and destroy every owned child;
the supervisor joins their Engine threads.

## Complete moonrun-owned virtualization inventory

The table is both a scope checklist and an ordering constraint. “Already
per-Run” means no new namespace is required, though Policy coverage may still
need strengthening.

| Surface | Current/likely coupling | End state | Earliest slice |
|---|---|---|---|
| Guest arguments | Run input | explicit per-Run immutable values | 1 |
| Environment get/set/unset and child inheritance | ambient process unless Policy supplies a map | mutable Env created for every Run, including legacy allow-all | 1 |
| Working directory and relative path base | process cwd | Host Filesystem cwd; never process `chdir` | 2 |
| Host-backed filesystem namespace | host paths plus permission checks | Policy-defined guest paths/mounts with handle-relative enforcement | 2 for cwd, 9 for portable namespace |
| Standard streams | process stdio | per-Run Host Stdio resources | 3 |
| Resources, Handles, Jobs, Workers, completions, pollers | Host/Async Host | remain per-Run | already |
| Run exit | former process exit risk | Run Termination outcome | already |
| Signal delivery and interruption | process signal compatibility | per-Run control channel; outer Adapter owns OS handlers | 3 |
| Execution attempts/status/wait/stop/destroy | direct synchronous Run calls | retained Execution Supervisor records | 4 |
| Declarative workload desired/observed state | none | Deployment Controller reconciliation | 5 |
| DNS, connect, bind | real OS network with Policy checks | per-Run authorization plus optional workload-name discovery | existing checks; discovery later |
| Process spawn/wait/handles/termination | native OS children | tagged native/virtual children in Host Process backed by Execution Supervisor | 7–8 |
| PID/parent-child identity | native process table | Host Process mapping for virtual children | 8 |
| Child env/cwd/stdio/Policy inheritance | native ambient behavior | explicit snapshot and attenuation from parent Run | 7–8 |
| Time | ambient OS clock | explicit ambient or virtual clock provider when required | 9 |
| Randomness | ambient OS RNG | explicit ambient or deterministic provider when required | 9 |

WASI descriptors remain outside this inventory for the first implementation.
WASI environment reads consume the current Env; runtime `set` and
`unset` operations are therefore visible on subsequent reads. A later WASI
filesystem design must either consume the same realized filesystem inputs or
state its intentionally different semantics.

## Agile delivery plan

Each slice is independently reviewable, keeps the compatibility Adapter, and
has an executable acceptance scenario. A slice should not introduce a general
interface until its second Adapter is present.

### Slice 0: integrate and characterize Engine

Deliverable:

- one Engine can compile a reusable Module and run it more than once;
- concurrent Runs have fresh Host, Guest Memory, and Run Termination state;
- Engine remains synchronous and caller-scheduled.

Acceptance:

- two concurrent executions of one Module cannot observe each other's Handles,
  Jobs, completion identifiers, or exit result.

### Slice 1: per-Run environment

Deliverable:

- split immutable Policy configuration from mutable Env;
- construct Env in legacy allow-all mode without eagerly enumerating the
  ambient environment;
- route every moonrun-owned environment get/set/unset and child environment
  builder through it;
- leave the process environment unchanged.

Acceptance:

- two concurrent Runs start with different values for the same variable;
- each mutates and unsets its value without affecting the other Run or the
  embedding process.

This is the first production-useful increment for embedders.

### Slice 2: per-Run working directory

Deliverable:

- Host Filesystem owns the Run's current directory and resolves every relative
  moonrun-owned path against it;
- no Run calls process `chdir`;
- the legacy Adapter snapshots the process cwd once.

Acceptance:

- two concurrent Runs open the same relative guest path under different host
  roots and receive different files;
- symlink and rename tests anchor authorization before a portable mount
  namespace is attempted.

### Slice 3: per-Run stdio and control

Deliverable:

- inject stdin/stdout/stderr resources into Host Stdio;
- integrate the existing per-instance signal/control channel with Engine;
- preserve inherited interactive streams through the CLI Adapter.

Acceptance:

- two concurrent Runs have separately captured output;
- terminating one Run wakes and tears it down without changing the other Run's
  output or outcome.

### Slice 4: Execution Supervisor MVP

Deliverable:

- start one Engine attempt from fully realized Run inputs;
- own its execution thread and control sender;
- retain observed state and terminal outcome independently from wait;
- keep terminate, wait, and destroy as distinct operations;
- disable automatic restart.

Acceptance:

- wait called after a fast Run has exited still receives the exact outcome;
- terminate stops one attempt by deadline without affecting another;
- destroy removes the retained record only after the attempt is terminal.

### Slice 5: Deployment Controller MVP

Deliverable:

- apply a declarative DeploymentSpec with at least two named workloads;
- converge each workload to one local Engine attempt through Execution
  Supervisor;
- report desired and observed state separately;
- delete the Deployment and deterministically stop/destroy all attempts;
- keep placement local and restart policy `Never`.

Acceptance:

- one command starts two workloads with conflicting env names and relative
  paths and observes both as running;
- externally changing desired state from present to absent removes one workload
  without terminating the native process or the other workload;
- a completed workload is observed as completed rather than silently restarted.

This is MVP A. It has the correct controller ownership without a Scheduler,
cluster protocol, discovery system, or runtime-injected binding.

### Slice 6: normalized process routing and execution Seam

Deliverable:

- move spawn authorization and route selection into Host Process;
- move the existing native execution mechanics behind Native Process Adapter;
- use Engine Run Adapter from Slice 4 as the second Adapter at the same Seam;
- add a Moonx route that can resolve a Module and child Run inputs without yet
  exposing it as a PID-shaped child;
- make both Adapters satisfy start, wait, stop, and destroy semantics.

Acceptance:

- a configured Moonx request selects the virtual Adapter;
- a denied request cannot fall back to PATH/native execution;
- existing native process tests remain unchanged through Native Process
  Adapter;
- wait-after-exit and stop-before-destroy hold for both Adapters.

### Slice 7: child inheritance and virtual execution

Deliverable:

- snapshot parent Env, working directory, and stdio routing when spawn is
  accepted;
- attenuate the parent Policy for the child;
- ask Execution Supervisor to start an Engine attempt with owner `parent child`;
- publish the supervisor's completion through the existing host completion
  mechanism;
- keep child restart policy `Never`.

Acceptance:

- a parent guest starts a child Module which observes the intended inherited
  environment/cwd, produces captured output, and exits independently;
- module resolution and execution errors have Moonx-shaped stderr and exit
  results.

### Slice 8: virtual child table MVP

Deliverable:

- add tagged native/virtual entries and guest identifier/Handle mappings to
  Host Process;
- implement spawn, wait, exit-status observation, one termination path, and
  parent teardown by delegating actual execution control to Execution
  Supervisor;
- keep all virtual identifiers out of OS process APIs.

Acceptance:

- an unmodified guest path that spawns the supported Moonx form can wait for a
  virtual child and receive its exit result;
- a virtual child may itself start a supported virtual child;
- concurrent parents cannot wait on or terminate each other's children;
- parent teardown leaves no running virtual-child thread.

This is MVP B.

### Slice 9: completeness and hardening

Possible follow-ups, selected by demonstrated need:

- portable guest filesystem paths and mount tables;
- stable workload discovery with native endpoint and direct in-process request
  Adapters, while preserving caller-owned request control flow;
- a Scheduler Seam only after a second placement target exists;
- virtual clocks and deterministic random providers;
- quotas, recursion/invocation limits, and cycle detection;
- `OnFailure`/`Always` workload restart, attempt history, health, and shutdown
  ordering in Deployment Controller;
- mixed native/virtual trees, detach, orphan, and repeated-wait semantics;
- optional worker-process isolation using Linux namespaces/cgroups, macOS
  sandbox facilities, or Windows restricted tokens/Job Objects;
- WASI descriptor and preopen integration as its own decision.

## Policy compatibility and required decisions

The target model changes two documented semantics and therefore requires an ADR
before implementation:

1. Moonrun Policy is currently defined as permission configuration rather than
   a virtual filesystem. The target design makes it the declaration of the
   complete moonrun-owned guest world while keeping mutable state in domain
   Modules.
2. Native process spawn is currently a coarse escape into ambient host
   authority. The target design makes native escape and virtual routing
   separate, explicit choices and attenuates authority for virtual children.

Compatibility is provided by an explicit legacy Adapter, not by direct ambient
reads throughout host Modules. With no Policy file, that Adapter provides the
embedding process environment as Env's lazy, read-only source, snapshots cwd,
borrows standard streams, and authorizes the same native operations as today.
With a Policy, the realized per-Run world is deny-by-default for omitted
moonrun-owned surfaces.

A future Policy schema should express the normalized model, not expose Engine
or OS implementation details. At minimum it needs independent declarations for
initial environment, guest cwd/path mappings, stdio modes, workload-name
network routes, process routes, native escape, child Policy attenuation, and
limits. The JSON spelling should be decided only after Slice 1 proves the
realization flow.

## Invariants

- Engine remains synchronous and owns no execution thread, retained status,
  desired state, or lifecycle policy.
- Deployment Controller owns desired workload state and reconciliation; it
  never calls Engine directly.
- Execution Supervisor owns concrete attempt threads, retained outcomes, wait,
  termination, and destruction; it does not interpret Deployment topology.
- Host Process owns guest-visible child identity, authority, wait eligibility,
  and reap semantics; it delegates actual execution lifecycle to Execution
  Supervisor.
- No moonrun-owned import mutates process environment or cwd. Guest environment
  reads occur only through Env's read-only source; cwd is snapshotted before
  Host construction. The current native escape Adapter may still use
  host-platform executable-lookup semantics until process routing is
  virtualized.
- Policy is immutable during a Run; mutable values live in their owning host
  Module.
- Child Policy is no more permissive than parent Policy.
- A route authorized as virtual never silently falls back to native execution.
- A virtual child identifier or process Handle is resolved by Host Process and
  never passed to an OS process API.
- Completion is event-driven; virtual child wait does not periodically poll.
- A virtual child is reported exited only after its Host teardown is complete.
- One parent Run cannot observe, wait for, signal, or reap another Run's child.
- A virtual child is never automatically restarted.
- Service discovery never creates, restarts, or keeps a workload alive.
- The compatibility Adapter is the only place that supplies ambient process
  state to host Modules.
- The goal is not to eliminate `cfg`; platform conditionals stay below OS
  Adapter seams, while shared Modules express platform-neutral semantics.

## Rejected alternatives

### Replace Engine with a process supervisor

Engine already has the correct deep responsibility: reusable compilation and
fresh synchronous Runs. Giving it service topology, threads, and child tables
would mix execution with lifecycle policy and make embedding harder.

### Change process globals around each Run

Temporarily changing environment or cwd cannot be made correct with concurrent
Runs or unrelated embedding threads. A global lock would serialize only callers
that cooperate with moonrun and would not protect other libraries.

### Use Linux namespaces for every Run

They are not portable, many operations require privileges, and their useful
filesystem/PID/resource semantics are tied to real tasks. They also conflict
with a V8 process that already owns threads. They remain valuable for an
optional worker-process Adapter.

### Create a second universal process tree Module

Host Process already owns native child authorization, Process Jobs, child PID
ownership, and process-handle provenance. A second table would create two
owners for wait/reap and authorization invariants.

### Fabricate an OS PID, fd, or HANDLE for a virtual child

Synthetic OS-looking values become dangerous as soon as one reaches a native
API. Moonrun should preserve the guest ABI through an internal mapping, not
pretend the operating system owns the object.

### Use Wrangler Service Bindings as the lifecycle model

Wrangler's runtime invokes a Worker and injects a callable binding into its
environment. That model is appropriate for an awaited request/RPC capability,
but it gives neither an external desired-state controller nor process-shaped
spawn/wait/reap ownership. Moonrun may later borrow its transport shape behind
discovery; it must not use it to own workloads or children.

### Put reconciliation and imperative child spawn in one mode-switched Module

A Deployment converges durable desired state, while a child spawn creates one
parent-scoped attempt with restart `Never`. Merging both into an
`apply_or_spawn` Interface hides incompatible lifetime rules. They remain
separate callers of Execution Supervisor.

## Open decisions

The following are intentionally deferred until their preceding slice provides
evidence:

- the exact guest-visible virtual child identifier allocation and reuse rule;
- the Moonx command forms and exit-code/stderr compatibility contract;
- whether a virtual child may outlive its parent;
- the workload discovery and request transport contract;
- the condition that justifies introducing a Scheduler Seam and Allocation
  identity;
- the portable guest path syntax and mount schema;
- which time, randomness, and resource-limit controls must be in the first
  complete Policy version;
- whether security requirements eventually require selected services or
  children to use a worker-process Adapter rather than an in-process Run.
