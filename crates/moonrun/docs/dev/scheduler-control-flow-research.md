# Scheduler Control-Flow Models for Moonrun

Status: research note, 2026-08-24.

## Executive answer

The earlier Wrangler analogy was wrong at the most important level: control-flow
ownership.

Wrangler's Service Binding model is runtime-injected and invocation-shaped. The
runtime invokes a Worker's handler for an incoming event and injects a callable
binding into the caller's `env`; calling another Worker is an awaited HTTP or
RPC invocation. Cloudflare explicitly says that an unawaited downstream Worker
may be terminated early. That is a useful precedent for an in-process request
binding, but not for a workload supervisor or a process tree
([Cloudflare Service Bindings](https://developers.cloudflare.com/workers/runtime-apis/bindings/service-bindings/),
[Cloudflare handler model](https://developers.cloudflare.com/workers/runtime-apis/handlers/)).

Kubernetes and Nomad have the control direction Moonrun needs:

- a caller declares intended work;
- an orchestrator owns desired and observed state;
- a node-local agent decides when actual execution must start, stop, or restart;
- an execution Adapter performs the runtime-specific operation and reports
  status upward; and
- workloads do not receive the orchestrator's lifecycle authority as an
  injected callable binding.

Kubernetes is the stronger precedent for `spec`/`status`, reconciliation,
owner-dependent cleanup, and separating a logical workload from a replaceable
execution attempt. Nomad is the closer implementation precedent for moonrun:
it is a single binary, treats an Allocation as the mapping of a task group to a
client, and gives task drivers explicit `StartTask`, `WaitTask`, `StopTask`,
`DestroyTask`, and recovery operations
([Kubernetes controllers](https://kubernetes.io/docs/concepts/architecture/controller/),
[Nomad scheduling](https://developer.hashicorp.com/nomad/docs/concepts/scheduling/how-scheduling-works),
[Nomad task-driver lifecycle](https://developer.hashicorp.com/nomad/plugins/author/task-driver)).

k0s does not supply a different scheduling or lifecycle model. It packages and
supervises certified upstream Kubernetes components in a single binary. Its
useful lesson for moonrun is packaging and local process supervision, not new
orchestration semantics
([k0s overview](https://docs.k0sproject.io/stable/),
[k0s architecture](https://docs.k0sproject.io/stable/architecture/)).

The resulting recommendation is:

1. Put multi-service desired state and reconciliation in a Deployment
   Controller above an Execution Supervisor. The controller owns convergence
   and restart policy; the supervisor owns concrete attempts, observed state,
   threads, and teardown.
2. Keep Engine synchronous. Place an execution Seam below the supervisor with
   at least an in-process Engine Adapter and a native-process Adapter. The
   supervisor calls this Interface; an Adapter reports completion back.
3. Represent a declared workload and a virtual child as two different lifetime
   policies over shared execution records. A workload is reconciled toward
   desired state. A child spawn is an imperative, parent-scoped request and
   defaults to no restart.
4. Keep the guest-visible child Handle/provenance in Host Process, but keep the
   authoritative execution record and attempt lifecycle in the supervisor.
   Host Process asks the supervisor to start, wait, signal, or terminate; it
   does not call Engine itself.
5. Treat service discovery as a separate name-to-endpoint Module. An eventual
   direct in-process request Adapter may optimize transport behind that Seam,
   but it must not invert lifecycle ownership.

## Question tree and hypotheses

The research used this MECE issue tree:

```text
Which reference model has the control ownership Moonrun needs?
├── A. Intent and convergence
│   ├── A1. Who records desired state?
│   ├── A2. Who observes actual state?
│   └── A3. Who decides to start, replace, restart, or stop?
├── B. Execution lifecycle
│   ├── B1. What is the scheduling/lifecycle unit?
│   ├── B2. Who invokes the runtime?
│   ├── B3. How are wait, exit, and recovery represented?
│   └── B4. How are termination and ownership cleanup represented?
├── C. Connectivity
│   ├── C1. How is a logical service named?
│   ├── C2. How is the current endpoint resolved?
│   └── C3. Is the workload called by injected capability or network endpoint?
└── D. Portability
    ├── D1. Is the control model OS-independent?
    ├── D2. Which worker platforms are implemented?
    └── D3. Which isolation features remain OS-specific?
```

The tested hypotheses were:

- **H1:** Wrangler is a suitable model for multi-service lifecycle ownership.
  **Rejected.** It is suitable for an injected request-binding contract, not
  for desired-state reconciliation or process-shaped execution.
- **H2:** Kubernetes supplies the correct external-control model, even if its
  distributed machinery is excessive for moonrun. **Supported.** Controllers
  reconcile resources, while kubelet reconciles node-local Pods through CRI.
- **H3:** k0s materially changes Kubernetes control flow and offers a separate
  model. **Rejected.** k0s preserves upstream Kubernetes and mainly changes
  packaging, bootstrap, defaults, and process supervision.
- **H4:** Nomad's client and task-driver model is a closer implementation shape
  for a cross-platform, single-binary moonrun. **Supported with caveats.** The
  lifecycle Interface is close, but driver isolation and networking vary by OS.
- **H5:** multi-service Runs and virtual children can use one identical
  reconciliation policy. **Rejected.** They can share execution records and
  Adapters, but a declared service is desired-state work whereas a child spawn
  is an imperative, parent-scoped lifetime with process-compatible wait and
  termination semantics.

## Evidence matrix

| Concern | Kubernetes | k0s | Nomad | Wrangler | Moonrun implication |
| --- | --- | --- | --- | --- | --- |
| Source of intent | Objects carry a desired `spec`; controllers move current state toward it. | Uses upstream Kubernetes objects and controllers. | A Job is declarative desired state; an Evaluation reconciles desired and emergent state. | Caller configuration declares a binding, but invocation starts from a runtime-delivered event or another Worker call. | Persist desired and observed service state outside Engine. Do not make injected bindings the topology owner. |
| Reconciliation owner | Control-plane controllers create/remove resources; kubelet has a node-local sync loop for assigned Pods. | k0s supervises the same API server, controller manager, scheduler, kubelet, and runtime roles. | Servers evaluate and place; clients watch Allocations and execute tasks. | Workers execute handlers when the runtime invokes them. | A Deployment Controller owns convergence; an Execution Supervisor owns concrete local attempts; Engine is below both. |
| Placement versus execution | Scheduler binds an unbound Pod to a Node; kubelet asks a CRI runtime to launch it. | Same Kubernetes division. | Server scheduler creates an Allocation mapping a task group to a client; the client uses a task driver. | No comparable node-agent lifecycle Interface is exposed by Service Bindings. | Preserve a placement/execution split even if MVP has one local placement. |
| Lifecycle unit | Pod is the smallest deployable object and groups co-scheduled containers. | Same Pod model. | Task Group is the scheduling unit; Allocation maps it to one client. A Task is the driver-executed unit. | An event/RPC invocation is the operative unit for bindings. | Use a logical workload/child record distinct from a concrete Run attempt. Do not identify a workload by a V8 isolate or thread. |
| Start | Kubelet reconciles a PodSpec and calls CRI to create/start the sandbox and containers. | Same, with bundled/default runtime wiring. | Client calls the task driver's `StartTask` with a stable task ID and receives a recoverable handle. | Runtime calls the Worker handler; one Worker calls another through injected `env` binding. | Supervisor starts an Adapter; Host Process and guests never call `Engine::run` directly. |
| Wait and status | Pod/container state is reported in object status; observers wait for status conditions. | Same. | `WaitTask` reports `ExitResult`; calling it after exit must return the recorded result immediately. | Downstream invocation is awaited as a call result; this is not a durable process status object. | Store terminal outcome in the Run record and make wait observe it through notification. Native `waitpid` stays inside its Adapter. |
| Stop and kill | Deletion expresses desired removal; kubelet requests graceful stop and force-kills after the grace period. | Same. | `StopTask` signals, waits for a timeout, then force-kills; `DestroyTask` removes retained task state. | A downstream invocation not kept alive by its caller may be terminated early. | Termination is a supervisor transition with a deadline. Engine Adapter maps it to Run termination/V8 interruption; native Adapter maps it to OS control. |
| Restart and replacement | Kubelet may restart containers in a Pod; higher controllers replace failed disposable Pods. A Pod is scheduled once and replacements have new UIDs. | Same. | Restarts occur on the client; exhausted local attempts can fail the Allocation and cause scheduler rescheduling. | Service Binding docs specify invocation lifetime, not long-lived desired-state restart policy. | Keep restart policy outside Engine. Give every attempt a distinct identity. Virtual children default to `Never`; services may opt into reconciliation. |
| Ownership cleanup | Owner references and garbage collection model dependent deletion. | Same. | Allocation and task-group lifecycle orders main, sidecar, prestart, and poststop tasks. | Binding declaration grants call access; it is not a process ownership tree. | Record parent-child ownership separately from service names and routing. Define parent exit, orphan, and cascade policy explicitly. |
| Service discovery | A Service is a stable logical network endpoint over changing Pods; clients discover it through DNS or environment variables. | Bundles Kubernetes networking and CoreDNS defaults. | Registers services with native discovery or Consul; consumers use addresses, templates, DNS, or a mesh. | Runtime injects a callable RPC/HTTP object into `env`; both Workers commonly execute on the same thread. | Start with stable names resolved to endpoint records. A later in-process call Adapter can preserve request/RPC semantics without owning Runs. |
| Platform story | Linux control plane; supported Linux and Windows workers. No native macOS worker is documented. | Upstream model; Windows worker support is experimental and requires Linux control plane plus a Linux worker. No macOS host support is listed. | Product docs state macOS, Windows, and Linux support; task-driver and isolation capabilities still vary by OS. | Cloudflare owns the production runtime; local tools abstract its host implementation. | Nomad is the better portability precedent, but moonrun still needs its own OS-neutral Interface and per-OS Adapters. |

Primary evidence:

- Kubernetes controller loops and the Job controller's explicit separation from
  Pod execution:
  [Controllers](https://kubernetes.io/docs/concepts/architecture/controller/).
- Kubernetes node execution ownership and CRI:
  [Cluster architecture](https://kubernetes.io/docs/concepts/architecture/),
  [Kubelet sync loop](https://kubernetes.io/docs/reference/node/kubelet-sync-loop/),
  and [CRI](https://kubernetes.io/docs/concepts/containers/cri/).
- Kubernetes workload identity, restart, and termination:
  [Pods](https://kubernetes.io/docs/concepts/workloads/pods/),
  [Pod lifecycle](https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/),
  [Jobs](https://kubernetes.io/docs/concepts/workloads/controllers/job/), and
  [owners and dependents](https://kubernetes.io/docs/concepts/overview/working-with-objects/owners-dependents/).
- Kubernetes logical service discovery:
  [Service](https://kubernetes.io/docs/concepts/services-networking/service/).
- Kubernetes platform limits:
  [Windows containers in Kubernetes](https://kubernetes.io/docs/concepts/windows/intro/)
  and
  [Windows resource management](https://kubernetes.io/docs/concepts/configuration/windows-resource-management/).
- k0s upstream status, architecture, runtime, and platform limits:
  [k0s overview](https://docs.k0sproject.io/stable/),
  [architecture](https://docs.k0sproject.io/stable/architecture/),
  [runtime](https://docs.k0sproject.io/stable/runtime/),
  [system requirements](https://docs.k0sproject.io/stable/system-requirements/),
  and
  [experimental Windows workers](https://docs.k0sproject.io/stable/experimental-windows/).
- Nomad desired-state scheduling and control-plane/client split:
  [architecture](https://developer.hashicorp.com/nomad/docs/architecture),
  [scheduling](https://developer.hashicorp.com/nomad/docs/concepts/scheduling/how-scheduling-works),
  and [glossary](https://developer.hashicorp.com/nomad/docs/glossary).
- Nomad execution and restart ownership:
  [task-driver authoring](https://developer.hashicorp.com/nomad/plugins/author/task-driver),
  [restart policy](https://developer.hashicorp.com/nomad/docs/job-specification/restart),
  and [task-group lifecycle](https://developer.hashicorp.com/nomad/docs/job-specification/lifecycle).
- Nomad service discovery and portability:
  [service discovery](https://developer.hashicorp.com/nomad/docs/networking/service-discovery),
  [task drivers](https://developer.hashicorp.com/nomad/docs/deploy/task-driver),
  [supported platforms](https://developer.hashicorp.com/nomad/docs/what-is-nomad),
  [raw-exec limitations](https://developer.hashicorp.com/nomad/docs/deploy/task-driver/raw_exec),
  and
  [Consul service-mesh platform limit](https://developer.hashicorp.com/nomad/docs/networking/consul/service-mesh).
- Wrangler's injected call model and local multi-Worker topology:
  [Service Bindings](https://developers.cloudflare.com/workers/runtime-apis/bindings/service-bindings/)
  and
  [multi-Worker development](https://developers.cloudflare.com/workers/local-development/multi-workers/).

## Findings

### 1. The decisive split is controller ownership versus runtime injection

Kubernetes controllers are non-terminating loops. They read declared resources,
act to move current state toward desired state, and report status. The Job
controller does not run containers; it creates Pods. Once a Pod is assigned to
a Node, kubelet becomes the node-local reconciliation owner and acts as the CRI
client
([Kubernetes controllers](https://kubernetes.io/docs/concepts/architecture/controller/),
[Kubelet sync loop](https://kubernetes.io/docs/reference/node/kubelet-sync-loop/)).

Nomad has the same direction with fewer layers. Users submit a Job. Servers
create Evaluations and Allocations. A client watches for assigned Allocations
and uses a task driver to run the tasks. The task cannot decide that it should
exist merely by holding an injected object
([Nomad architecture](https://developer.hashicorp.com/nomad/docs/architecture),
[Nomad scheduling](https://developer.hashicorp.com/nomad/docs/concepts/scheduling/how-scheduling-works)).

Wrangler Service Bindings point the other way for invocation. The caller gets a
binding on `env` and invokes the target with RPC or HTTP. The target's work is
tied to that awaited invocation; Cloudflare warns that failing to await it can
terminate it early. This is a capability-bearing call path, not an external
desired-state controller
([Cloudflare Service Bindings](https://developers.cloudflare.com/workers/runtime-apis/bindings/service-bindings/)).

**Finding:** moonrun's multi-service topology needs an external Deployment
Controller. Wrangler remains relevant only if moonrun later injects a
request/RPC binding after the controller and Execution Supervisor have already
established the target's lifecycle and route.

### 2. Nomad's execution Seam is closer than CRI to a virtual Engine Run

CRI establishes the correct separation: kubelet is the client and the runtime
is an Adapter. It is nevertheless container-specific. Nomad's task-driver
Interface is closer to the operations moonrun must expose:

- `StartTask` accepts all launch configuration, assigns a stable ID, and
  returns state usable for recovery;
- `WaitTask` reports exit and must still report a retained exit after the task
  has already finished;
- `StopTask` performs graceful control followed by forced termination after a
  timeout;
- `DestroyTask` removes retained task state independently from stop; and
- `InspectTask` reports current status
  ([Nomad task-driver lifecycle](https://developer.hashicorp.com/nomad/plugins/author/task-driver)).

This separation prevents three common ownership errors:

1. execution completion does not erase state before a waiter observes it;
2. stop is not conflated with resource destruction; and
3. a restarted client or driver can recover an already-running task rather
   than treating the supervising process as the task's lifetime.

Moonrun does not need to copy the plugin protocol. It should borrow the
Interface semantics at an internal Seam. The Engine Adapter can implement
`start` by placing synchronous `Engine::run` on a supervisor-owned thread and
can implement `wait` from the stored Run outcome. A native-process Adapter can
use OS spawn, wait, and termination internally.

**Finding:** Engine remains an execution kernel. The Execution Supervisor owns
the execution thread, handle registry, status retention, and control requests.

### 3. Logical identity and execution-attempt identity must differ

A Kubernetes Pod has a UID, is scheduled once, and is not moved to another
Node. If a higher-level workload needs recovery, its controller creates a
replacement Pod with a new identity. Kubelet may separately restart a
container within the same Pod according to policy
([Kubernetes Pod lifecycle](https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/)).

Nomad similarly distinguishes a Job, a Task Group, an Allocation that maps the
group to one client, and the task execution controlled by a driver. Local task
restart and scheduler rescheduling are different operations
([Nomad glossary](https://developer.hashicorp.com/nomad/docs/glossary),
[Nomad restart policy](https://developer.hashicorp.com/nomad/docs/job-specification/restart)).

**Finding:** moonrun needs at least two identities in the model:

- a stable workload or child identity that callers observe; and
- a Run-attempt identity for one concrete Engine invocation.

For a virtual child, the first MVP should have exactly one attempt and restart
policy `Never`, preserving child-process expectations. For a declared service,
later restart or replacement creates a new attempt while the service identity
stays stable. A V8 isolate, thread ID, native PID, or virtual PID must not serve
as both identities.

### 4. Service discovery is downstream of lifecycle, not its owner

Kubernetes Service provides a stable network identity while controllers update
EndpointSlices as Pods change. Consumers use DNS or environment variables.
Nomad registers addresses with its native catalog or Consul and exposes them
through queries, templates, DNS, or a mesh. In both systems, discovery observes
the currently realized workloads; it does not own their execution
([Kubernetes Service](https://kubernetes.io/docs/concepts/services-networking/service/),
[Nomad service discovery](https://developer.hashicorp.com/nomad/docs/networking/service-discovery)).

Wrangler combines discovery, authorization, and transport in a callable object
injected into `env`. That can be attractive for in-process Runs, but importing
the binding before the scheduler model would invert ownership again.

**Finding:** discovery should resolve a logical name to current healthy
endpoints or a request route, populated from observed workload state. Introduce
its Seam only when native endpoint routing and in-process dispatch are both
real Adapters. Until then, keep discovery out of the execution Interface.

### 5. Kubernetes semantics are portable; its implementation is not

Kubernetes supports Linux or Windows worker nodes, but its control plane runs
on Linux. The Windows worker implementation has feature differences, and its
resource control uses Windows Job Objects where Linux uses cgroups. The
reviewed upstream documentation does not define a native macOS worker
([Kubernetes Windows overview](https://kubernetes.io/docs/concepts/windows/intro/),
[Windows resource management](https://kubernetes.io/docs/concepts/configuration/windows-resource-management/)).

k0s preserves these constraints. Its Windows worker support is explicitly
experimental, requires a Linux control plane and at least one Linux worker, and
supervises `kubelet.exe` and `kube-proxy.exe`. Its published system requirements
list Linux and Windows, not macOS
([k0s experimental Windows](https://docs.k0sproject.io/stable/experimental-windows/),
[k0s system requirements](https://docs.k0sproject.io/stable/system-requirements/)).

Nomad explicitly supports macOS, Windows, and Linux and exposes driver
capabilities so placement can account for installed drivers. That does not make
isolation uniform. Its `raw_exec` driver runs on supported operating systems
but provides no filesystem isolation; Linux can add cgroups, while Nomad's
Consul service mesh requires Linux network namespaces and does not run on
Windows or macOS
([Nomad supported platforms](https://developer.hashicorp.com/nomad/docs/what-is-nomad),
[Nomad raw exec](https://developer.hashicorp.com/nomad/docs/deploy/task-driver/raw_exec),
[Nomad Consul service mesh](https://developer.hashicorp.com/nomad/docs/networking/consul/service-mesh)).

**Finding:** borrow Kubernetes' OS-neutral desired/observed model and Nomad's
capability-reporting execution Seam. Do not claim Kubernetes, k0s, or Nomad can
provide portable isolation for in-process Runs. Moonrun still needs its own
Linux, macOS, and Windows backing implementations.

## Recommended Moonrun control model

The following names are descriptive for this research note, not accepted
Moonrun glossary entries.

```text
Deployment Spec ──desired state──▶ Deployment Controller
                                           │
                                           ├─ reconcile managed workloads
                                           ├─ own restart/replacement policy
                                           └─ publish workload status
                                                       │
Guest spawn Job ─▶ Host Process ───────────────┐        │
Guest wait/kill ─▶ Host Process ───────────────┤        │
                                              ▼        ▼
                                         Execution Supervisor
                                           ├─ retain observed status/outcome
                                           └─ own attempts, threads, deadlines,
                                              wait, termination, and teardown
                                                       │
                                             execution Interface
                                                ┌──────┴──────┐
                                                ▼             ▼
                                      Engine Run Adapter  Native Process Adapter
                                                │             │
                                                ▼             ▼
                                          Engine::run    OS spawn/wait/kill
```

This puts the Seams at places where at least two Adapters genuinely vary:

- the execution Seam has Engine and native-process Adapters;
- the discovery Seam can have native endpoint and future in-process request
  Adapters; and
- the platform backing below each Adapter differs across Linux, macOS, and
  Windows without leaking those differences into Policy or guest Handles.

Deployment Controller and Execution Supervisor have different Interfaces
because desired-state reconciliation and imperative execution are different
control flows:

```text
Deployment Controller
  apply(deployment_spec) -> revision
  observe(deployment_ref) -> deployment_status
  delete(deployment_ref) -> revision

Execution Supervisor
  start(owner, execution_spec) -> execution_ref
  observe(execution_ref) -> execution_status
  wait(execution_ref) -> outcome
  terminate(execution_ref, deadline, reason)
  destroy(execution_ref)
```

The controller calls the supervisor while reconciling a Deployment. Host
Process calls the same supervisor after authorizing an imperative child
request. The supervisor does not need a mode-switched `apply_or_spawn`
operation and does not own service topology.

The execution Interface can mirror the useful parts of Nomad without exposing
driver implementation details:

```text
start(run_attempt_spec) -> attempt_handle
wait(attempt_handle) -> outcome
stop(attempt_handle, deadline, reason)
destroy(attempt_handle)
```

`destroy` is intentionally distinct from `stop` and `wait`: waiters must still
observe an outcome after execution ends, and Handle cleanup must be explicit.

### Managed service path

A declared workload has desired and observed state. Deployment Controller
compares the two and asks Execution Supervisor to start, stop, or later replace
attempts. Workload identity remains stable across attempts. The first
implementation can have one local placement, one replica per declaration, no
health-based restart, and restart policy `Never`; that still establishes
correct ownership without implementing a distributed scheduler.

### Virtual child path

A virtual child is not a miniature Deployment and should not be silently
restarted. The guest's spawn Job is an imperative request. Host Process checks
Policy and guest Handle provenance, then asks Execution Supervisor to
materialize one execution record and one attempt. The supervisor owns the
execution; Host Process owns the parent guest's reference to it.

The child outcome remains queryable until all guest-visible Handles and pending
waits are released. Termination changes the record to a stopping state and asks
the execution Adapter to stop. A virtual process identifier is never sent to
an OS process operation.

Parent-child ownership and service discovery are independent relations:

- ownership determines cascading termination, orphan behavior, wait
  eligibility, and Policy attenuation;
- service identity determines request routing; and
- attempt identity determines which concrete execution produced a status or
  consumed resources.

## Agile implications

The research changes the order and acceptance criteria, not the goal of
starting with shared virtualization:

1. **Per-Run inputs:** explicit environment, cwd, stdio, Policy, and control
   remain the shared first slice. They are required by every execution Adapter.
2. **Execution Supervisor skeleton:** add one local observed-state registry and
   one Engine Adapter. It owns threads and retains terminal outcomes; restart
   is disabled.
3. **Multi-service MVP:** add Deployment Controller, apply a declarative
   specification with several named workloads, converge each to one local Run,
   observe status, and stop all Runs deterministically. No distributed
   placement or callable binding is needed.
4. **Execution Seam:** add the native-process Adapter or a test Adapter before
   generalizing the Interface further. Two concrete Adapters justify the Seam.
5. **Virtual-child MVP:** route one authorized Moonx spawn through Host Process
   to the supervisor; implement spawn, wait-after-exit, terminate with deadline,
   and explicit cleanup; keep restart `Never`.
6. **Reconciliation hardening:** add health, `OnFailure`/`Always` service restart,
   attempt history, and shutdown ordering.
7. **Discovery and transport:** publish stable logical service endpoints, then
   add direct in-process request dispatch only behind the same routing
   Interface.
8. **Platform hardening:** let execution Adapters report supported isolation and
   control capabilities; use platform-specific mechanisms without changing the
   supervisor model.

## Explicit caveats

- This research recommends Kubernetes' control model, not its API-server,
  distributed consensus, admission, container networking, or cluster scale.
  A single-process MVP does not need to imitate those mechanisms.
- Nomad's task-driver Interface is an analogy, not a mandate to introduce an
  out-of-process plugin protocol or copy its exact types.
- Neither Kubernetes nor Nomad models a POSIX parent-child tree for ordinary
  application code. Virtual child inheritance, wait ownership, orphaning, and
  signal behavior still require Moonrun-specific design and native-behavior
  tests.
- `spec`/`status` is useful for managed services. Treating `spawn` as durable
  desired state would incorrectly permit automatic child recreation; virtual
  children need imperative creation plus retained observed state.
- Nomad's statement that it supports macOS, Windows, and Linux does not imply
  equal isolation on those platforms. The documented raw-exec and service-mesh
  limitations are counterexamples.
- k0s marketing language about a single package or broad infrastructure support
  must not be read as symmetric host support. Its dedicated Windows document is
  explicitly experimental and Linux-dependent.
- Wrangler remains a valid precedent for a declared, capability-bearing,
  in-process request Interface. It is rejected only as the owner of deployment
  and process lifecycle.
- The provisional terms in this note need a glossary/ADR decision before they
  become public Moonrun vocabulary or implementation types.
