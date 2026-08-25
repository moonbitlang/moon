# OS support for Moonrun execution virtualization

Status: research note, 2026-08-24

## Executive answer

Moonrun has two target scenarios:

1. reconcile several declared workloads under one reusable
   `Engine`/compiled-`Module` execution substrate; and
2. intercept a guest `moonx` spawn and represent another in-process Engine run
   as a child process.

The operating system boundary is decisive:

- **For a real child process, the OS already supplies most of the desired
  virtualization.** POSIX `posix_spawn()` accepts a child environment and can
  control file descriptors, working directory, process group, and signals;
  Windows `CreateProcess()` accepts a child environment and working directory
  and controls inherited handles. Linux adds mount/network/PID namespaces,
  pidfds, Landlock, and cgroup v2. Windows adds AppContainer and Job Objects.
  macOS supplies strong spawn, `kqueue`, process-group, and `rlimit` primitives,
  but the reviewed public APIs do not expose Linux-style mount/network/PID
  namespaces or a Windows Job Object equivalent.
- **For an in-process Engine run, the OS does not create a second process
  world.** Environment, address space, security context, process identity, and
  hard memory limits remain properties of the enclosing process. cwd and file
  descriptor state are also process-wide in portable POSIX and Windows APIs.
  Therefore the Moonrun Policy/Host boundary must own the guest-visible env,
  cwd/path resolution, stdio, process identity, wait state, cancellation,
  process tree, service routing, networking permissions, and per-run quotas.
- **The useful OS reuse inside one process is at the backing-mechanism layer.**
  Pipes and sockets can back virtual stdio; `eventfd`/a pipe, `EVFILT_USER`, or
  `PostQueuedCompletionStatus()` can wake the existing poller; real sockets can
  back explicitly authorized network Resources. V8's
  `Isolate::TerminateExecution()` can interrupt a virtual child without
  terminating Moonrun.
- **Linux per-thread facilities are partial, not a portable semantic base.**
  `unshare(CLONE_FS)` can give one thread private cwd/root/umask and
  `CLONE_FILES` a private descriptor table. Mount and network namespaces can
  also be task-associated. These mechanisms stop matching the Engine once host
  Jobs execute on shared Workers or V8 performs background work. Creating an
  unprivileged user namespace additionally requires the caller not to be
  threaded. cgroup v2 thread mode can account/limit CPU for a dedicated runtime
  thread, but not memory, and cannot kill only one thread-group member.

The design implication is to deepen the existing per-run Host and process
domain, not to emulate an OS beside them. Both scenarios should first share
explicit per-Run inputs and owning host domains for Policy, environment,
virtual cwd/root, stdio, and cancellation. Deployment reconciliation and
service discovery sit above that substrate with their own control flow. Only
the child-process scenario needs the later, special mapping from a virtual
process ID/Handle to an Engine run.

## Scope and method

This is hypothesis-driven research. It separates evidence from recommendations
and uses the following MECE issue tree:

```text
Can the OS supply the execution boundary?
├── A. Per-run visible state
│   ├── A1. environment
│   ├── A2. cwd and filesystem namespace
│   └── A3. stdin/stdout/stderr
├── B. Process-shaped lifecycle
│   ├── B1. identity and stable handles
│   ├── B2. wait/completion/poll
│   ├── B3. signals and cancellation
│   └── B4. groups, descendants, and cleanup
└── C. External resources
    ├── C1. networking and service-to-service calls
    └── C2. resource accounting and limits
```

The hypotheses tested were:

- **H1:** real child processes can delegate most of A, B, and C to the OS;
- **H2:** in-process Engine runs can delegate notification and I/O backing to
  the OS, but not their identity or policy-visible world;
- **H3:** Linux per-thread APIs are useful enough to replace explicit per-run
  state; and
- **H4:** the common part of the two use cases ends before process identity.

H1, H2, and H4 are supported. H3 is rejected as a cross-platform design and is
only partially supported as an optional Linux optimization.

This note concerns Moonrun-owned imports and the Moonrun Policy. It does not
claim equivalent WASI descriptor isolation.

## Decision matrix

Legend:

- **OS process**: the kernel supplies the semantics for a real child;
- **OS backing**: an OS object is useful, but Moonrun still owns guest semantics;
- **Moonrun**: there is no suitable per-Engine OS object; virtualize explicitly.

| Capability | Real child process | In-process Engine run | Primary owner for the proposed design |
| --- | --- | --- | --- |
| Environment | **OS process**: `envp` / `lpEnvironment` | **Moonrun**: host environment is process-wide | mutable per-run env map initialized before execution |
| cwd | **OS process**: spawn-time cwd | **Moonrun**; Linux `CLONE_FS` is partial | virtual cwd plus dir-relative path operations |
| Filesystem view | Linux namespace/Landlock; Windows AppContainer; otherwise child credentials/path setup | **Moonrun**; Linux mount namespace is partial and non-portable | Host Filesystem namespace/mount table |
| stdio | **OS process**: descriptor/handle inheritance and redirection | **OS backing** plus **Moonrun** routing | injected stdin/stdout/stderr Resources |
| Process identity | PID or Windows process object/handle | **Moonrun** | virtual process ID plus existing Handle namespace |
| Wait and exit | `waitpid`, pidfd, `EVFILT_PROC`, waitable Windows process handle | **OS backing** for wakeup; status is **Moonrun** state | virtual child state machine and Completion Queue |
| Signal/cancel | Unix signals/process groups; Windows console control/termination | **Moonrun**, with V8 interruption | cancellation/termination channel per run |
| Tree cleanup | Linux process groups/cgroups/PID namespaces; Windows Job Objects; macOS process groups | **Moonrun** | virtual parent/child tree and teardown policy |
| Networking | Linux net namespace/Landlock; Windows AppContainer; real sockets elsewhere | **OS backing** plus **Moonrun** authorization/routing | socket Resources and workload-name discovery/routing |
| Resource limits | Linux rlimit/cgroup; macOS rlimit; Windows Job Object | mostly **Moonrun**; Linux thread CPU control is partial | concurrency, memory, time, and operation quotas |

## Evidence

### A1. Environment

For real children, POSIX `posix_spawn()` takes an explicit `envp`. The current
POSIX specification says the child is constructed with the supplied argument
and environment lists; its rationale treats environment, file descriptors,
cwd, process group, and signals as spawn inheritance controls
([The Open Group: `posix_spawn`](https://pubs.opengroup.org/onlinepubs/9799919799/functions/posix_spawn.html)).
Windows documents that every process has an environment block and that a
different block can be supplied to `CreateProcess()`
([Microsoft: Environment Variables](https://learn.microsoft.com/en-us/windows/win32/procthread/environment-variables),
[Microsoft: `CreateProcess`](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessa)).

For in-process runs, POSIX exposes the environment through the process-global
`environ`. POSIX explicitly forbids concurrent environment access while another
thread modifies it and does not require `setenv()`/`unsetenv()` to be
thread-safe
([The Open Group: general thread-safety rules](https://pubs.opengroup.org/onlinepubs/9799919799/functions/V2_chap02.html),
[The Open Group: `exec` and `environ`](https://pubs.opengroup.org/onlinepubs/9799919799/functions/exec.html)).
Windows likewise defines one environment block per process
([Microsoft: About Processes and Threads](https://learn.microsoft.com/en-us/windows/win32/procthread/about-processes-and-threads)).
No reviewed API supplies an environment block attached to a V8 Isolate or an
arbitrary logical run.

**Finding:** an explicit env map is both portable and shared by multi-service
runs and virtual children. Temporarily swapping the host environment is not a
valid concurrent implementation.

### A2. cwd and filesystem namespace

For real children, POSIX.1-2024 spawn file actions track a child working
directory without requiring the multi-threaded parent to change its own cwd
([The Open Group: `posix_spawn`](https://pubs.opengroup.org/onlinepubs/9799919799/functions/posix_spawn.html)).
Apple documents `posix_spawn()` as constructing a distinct process with
controlled file actions and inherited process attributes
([Apple: `posix_spawn(2)`](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/posix_spawn.2.html)).
Windows `CreateProcess()` accepts `lpCurrentDirectory`; Microsoft warns that
`SetCurrentDirectory()` changes the single cwd shared by every thread
([Microsoft: `CreateProcess`](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessa),
[Microsoft: `SetCurrentDirectory`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setcurrentdirectory)).
Apple's Foundation documentation likewise calls cwd a process property and
warns it may be changed by any thread
([Apple: `URL.currentDirectory()`](https://developer.apple.com/documentation/foundation/url/currentdirectory%28%29?language=occ)).

Portable POSIX `*at` operations provide the important in-process building
block: `openat()` resolves a relative path from a directory descriptor instead
of process cwd. The POSIX rationale explicitly names a virtual per-thread cwd
as a motivation
([The Open Group: `open`/`openat`](https://pubs.opengroup.org/onlinepubs/9799919799/functions/open.html),
[The Open Group portability rationale](https://pubs.opengroup.org/onlinepubs/9699919799/xrat/V4_port.html)).
Linux `openat2()` goes further: `RESOLVE_BENEATH` prevents escape above a
dirfd, and `RESOLVE_IN_ROOT` treats the dirfd as a temporary root for one path
resolution
([Linux man-pages: `openat2(2)`](https://man7.org/linux/man-pages/man2/openat2.2.html)).
This is path-resolution help, not a complete filesystem namespace; every
Moonrun filesystem operation must still use the same root/cwd discipline.

Linux offers two stronger families:

- `unshare(CLONE_FS)` detaches the calling thread's root, cwd, and umask, while
  `CLONE_FILES` detaches its descriptor table. `CLONE_NEWNS` creates a private
  mount namespace and implies `CLONE_FS`; `CLONE_NEWNET` moves the caller into
  a new network namespace. These operations normally require `CAP_SYS_ADMIN`
  ([Linux man-pages: `unshare(2)`](https://man7.org/linux/man-pages/man2/unshare.2.html)).
- A mount namespace isolates the list of mounts visible to its member
  processes
  ([Linux man-pages: mount namespaces](https://man7.org/linux/man-pages/man7/mount_namespaces.7.html)).
  Landlock can restrict filesystem and network ambient rights for the current
  thread and its descendants without privilege, but the restriction can only
  become tighter and cannot be removed
  ([Linux kernel: Landlock userspace API](https://www.kernel.org/doc/html/latest/userspace-api/landlock.html)).

The Linux options do not form a safe general in-process Engine boundary:

- an Engine run's host Jobs may execute on shared Workers that did not enter
  the runtime thread's namespace or detached fd table;
- newly created V8/background threads inherit from the creating thread rather
  than from a logical Engine identifier;
- `CLONE_NEWUSER`, the usual way for an unprivileged caller to acquire
  capabilities for other new namespaces, requires the calling process not to
  be threaded
  ([Linux man-pages: `unshare(2)`](https://man7.org/linux/man-pages/man2/unshare.2.html)); and
- Landlock is intentionally irreversible, making worker reuse across
  differently privileged runs unsuitable.

Windows AppContainer can isolate a real process's files, network, credentials,
and process access, but it is a process launch/security-token facility, not an
Isolate facility
([Microsoft: AppContainer isolation](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation),
[Microsoft: launching an AppContainer](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer)).

**Finding:** use OS cwd/namespaces for real native children where available.
For Engine runs, resolve all Moonrun-owned paths through explicit virtual
cwd/root state. On Linux, use `openat2()` as a hardening implementation detail,
not as the portable policy model.

### A3. stdin, stdout, and stderr

POSIX spawn file actions can close, open, and `dup2` descriptors in the child;
unchanged descriptors otherwise remain inherited
([The Open Group: `posix_spawn`](https://pubs.opengroup.org/onlinepubs/9799919799/functions/posix_spawn.html)).
Windows uses `STARTF_USESTDHANDLES` for standard handles and can restrict child
inheritance with `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`
([Microsoft: process inheritance](https://learn.microsoft.com/en-us/windows/win32/procthread/inheritance)).

Those operations change a real child's descriptor/handle table. They cannot
give two Engine runs different meanings for process fd 0/1/2. Pipes, sockets,
files, and memory buffers can still back per-run streams. On Windows,
overlapped named-pipe I/O can deliver completion through an IOCP
([Microsoft: overlapped pipe I/O](https://learn.microsoft.com/en-us/windows/win32/ipc/synchronous-and-overlapped-input-and-output)).
On Unix, pipe/socket fds are pollable; on macOS `EVFILT_READ` and
`EVFILT_WRITE` cover fd readiness
([Apple: `kqueue(2)`](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/kqueue.2.html)).

**Finding:** stdio should be three injected Resource-like endpoints. OS pipes
are an implementation option, not their guest identity. This state is shared
by both target scenarios.

### B1. Process identity and stable handles

A real POSIX child receives a kernel PID and is waitable by its parent. Linux
pidfds provide a stable fd reference to a process, are observable by
`poll`/`epoll`, can be waited on with `waitid(P_PIDFD)`, and avoid PID-reuse
races when signaling
([Linux man-pages: `pidfd_open(2)`](https://man7.org/linux/man-pages/man2/pidfd_open.2.html),
[Linux man-pages: `pidfd_send_signal(2)`](https://man7.org/linux/man-pages/man2/pidfd_send_signal.2.html)).
macOS `EVFILT_PROC` takes a PID and can report `NOTE_EXIT`, `NOTE_FORK`,
`NOTE_EXEC`, and `NOTE_REAP`
([Apple: `kqueue(2)`](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/kqueue.2.html)).
Windows returns a PID and process Handle. The process object becomes signaled
at termination and can be passed to the wait functions
([Microsoft: terminating a process](https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process),
[Microsoft: `WaitForSingleObject`](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject)).

Linux PID namespaces only assign different PID mappings to actual processes;
the first actual process is PID 1, and subsequent `fork`/`clone` children
receive namespace PIDs
([Linux man-pages: PID namespaces](https://man7.org/linux/man-pages/man7/pid_namespaces.7.html)).
They cannot assign a second PID to a logical run inside an existing process.
The same is true of macOS PIDs and Windows process objects.

**Finding:** a virtual child requires a Moonrun-allocated ID and an entry in a
virtual process table. It must never pass that ID to `kill`, `waitpid`,
`OpenProcess`, or other native APIs. The current process-policy state is built
around OS PIDs and process handles
([`async_host/mod.rs`](../../src/async_host/mod.rs)); this is the special seam
that appears only after the shared per-run context exists.

### B2. wait, completion, and poll integration

Real child exit already fits every target OS event mechanism:

- Linux pidfds become readable on process exit and can be registered with
  `epoll`
  ([Linux man-pages: `pidfd_open(2)`](https://man7.org/linux/man-pages/man2/pidfd_open.2.html));
- macOS provides `EVFILT_PROC/NOTE_EXIT`, with `waitpid()` still retrieving and
  reaping child status
  ([Apple: `kqueue(2)`](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/kqueue.2.html),
  [Apple: `wait(2)`](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/wait.2.html)); and
- Windows process handles are waitable; Job Object events can also be
  associated with an IOCP
  ([Microsoft: Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
  [Microsoft: Nested Jobs](https://learn.microsoft.com/en-us/windows/win32/procthread/nested-jobs)).

A virtual child's completion is a user-space state transition, but it can wake
the same poller:

- Linux `eventfd()` creates a pollable event-notification fd
  ([Linux man-pages: `eventfd(2)`](https://man7.org/linux/man-pages/man2/eventfd.2.html));
- Darwin exposes `EVFILT_USER` and `NOTE_TRIGGER`
  ([Apple XNU `event.h`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/event.h)); and
- Windows explicitly permits application-generated IOCP packets through
  `PostQueuedCompletionStatus()`
  ([Microsoft: `PostQueuedCompletionStatus`](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-postqueuedcompletionstatus)).

Moonrun already has epoll, kqueue, and IOCP Host Poller adapters
([`poll/mod.rs`](../../src/async_sys/internal/event_loop/poll/mod.rs)) and a
Completion Queue abstraction. Therefore no new public wait mechanism is
required for virtual children.

**Finding:** translate Engine completion into the existing host Completion
Queue and wake its existing completion source. `eventfd`/`EVFILT_USER`/IOCP are
optional backing choices; they are not virtual process handles.

### B3. Signals and cancellation

Unix signals and process groups control real processes. POSIX process groups
exist to signal related processes; macOS exposes `killpg()`
([The Open Group: process-group definition](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap03.html),
[Apple: `killpg(2)`](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/killpg.2.html)).
Linux pidfd signaling avoids PID reuse. Windows console process groups can
receive `CTRL+BREAK`, subject to console attachment rules, and
`TerminateProcess()` terminates a real process
([Microsoft: `GenerateConsoleCtrlEvent`](https://learn.microsoft.com/en-us/windows/console/generateconsolectrlevent),
[Microsoft: terminating a process](https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process)).

These mechanisms cannot terminate one Engine run without terminating or
signaling threads that share the Moonrun process. POSIX asynchronous thread
cancellation is especially unsuitable as a general substitute: only three
functions are required to be async-cancel-safe, and cancellation during any
other function has undefined behavior
([The Open Group: thread cancellation](https://pubs.opengroup.org/onlinepubs/9799919799/functions/V2_chap02.html)).

V8 does provide an embedder-level primitive: `Isolate::TerminateExecution()`
may be called from another thread and forcefully terminates JavaScript
execution in that Isolate
([V8 public API: `v8-isolate.h`](https://chromium.googlesource.com/v8/v8.git/%2B/HEAD/include/v8-isolate.h)).
Host Jobs that are already blocking still need their own cooperative
cancellation or resource close/abort path.

**Finding:** represent guest signals as virtual termination requests. The
outer CLI may translate Ctrl-C/OS signals into the selected run's cancellation
channel; only a native child adapter should invoke native signal APIs.

### B4. Process grouping and cleanup

The OS can group real descendants, with different strength on each platform:

- POSIX process groups support group signaling, but membership can change and
  they do not provide resource accounting. macOS supplies this level directly
  through `setpgid()`/`killpg()`
  ([Apple: `setpgid(2)`](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/setpgid.2.html)).
- Linux PID namespaces give a namespace an init/reaper; if that init exits,
  the kernel kills the namespace's remaining processes
  ([Linux man-pages: PID namespaces](https://man7.org/linux/man-pages/man7/pid_namespaces.7.html)).
  cgroup v2's `cgroup.kill` kills all processes in a cgroup subtree and handles
  concurrent forks/migration
  ([Linux kernel: cgroup v2](https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html)).
- Windows Job Objects manage a group of processes as a unit, normally include
  descendants, can terminate the group, and support
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Nested jobs model process trees on
  Windows 8+
  ([Microsoft: Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)).

None of these kernels sees relationships between two Engine runs in one
process. Closing a thread Handle does not terminate the thread, and terminating
the Moonrun process would terminate every run.

**Finding:** keep the virtual parent/child relation in Moonrun. It should own
whether non-detached descendants are cancelled during parent/Host teardown.
Real-child adapters may additionally use cgroups, process groups, or Job
Objects for kernel-enforced cleanup.

### C1. Networking and multi-service routing

Linux network namespaces isolate devices, protocol stacks, routing tables,
firewall rules, port numbers, and abstract Unix sockets
([Linux man-pages: network namespaces](https://man7.org/linux/man-pages/man7/network_namespaces.7.html)).
Landlock can restrict selected network actions/ports for a thread and its
descendants. Windows AppContainer can deny network access unless appropriate
capabilities are granted. These mechanisms are valuable for real sandboxed
processes but have the same task/process association problems described above
for an in-process Engine.

Networking does not determine workload lifecycle ownership. Kubernetes and
Nomad publish stable logical service names over changing workload endpoints;
the consumer still initiates an ordinary network or request flow. Wrangler's
callable Service Binding instead injects an invocation capability into a
runtime-invoked Worker. The control-flow comparison is developed separately in
[Scheduler Control-Flow Models for Moonrun](scheduler-control-flow-research.md).

**Finding:** keep deployment reconciliation, service discovery, and request
transport as separate Modules. Host Network may later resolve a declared name
to a native endpoint or direct in-process request route, but discovery must not
create, restart, or keep a workload alive. OS sockets remain appropriate for
external ingress/egress and compatibility surfaces.

### C2. Resource accounting and limits

Real process limits are mature:

- Unix `rlimit` values are process attributes inherited by children and shared
  by all threads; Linux `prlimit()` can change another process's limits
  ([Linux man-pages: `getrlimit(2)`](https://man7.org/linux/man-pages/man2/getrlimit.2.html)).
  macOS documents `RLIMIT_CPU`, file-size, data, and other limits for the
  current process and processes it creates
  ([Apple: `getrlimit(2)`](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/getrlimit.2.html)).
- Linux cgroup v2 accounts and controls groups of processes, including CPU,
  memory, and PID controllers. `memory.max` is a hard memory limit and
  `cgroup.kill` operates on the process subtree
  ([Linux kernel: cgroup v2](https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html)).
- Windows Job Objects enforce and account for process/job memory, CPU rate,
  process count, and other limits
  ([Microsoft: Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
  [Microsoft: extended Job limits](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_extended_limit_information),
  [Microsoft: Job CPU rate control](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_cpu_rate_control_information)).

Those limits cannot generally distinguish Engine runs in one address space.
Linux cgroup v2 is a narrow exception: thread mode permits thread-granularity
control for the `cpu`, `cpuset`, `perf_event`, and `pids` controllers. Domain
consumption such as shared process memory remains at the common threaded
domain, and `cgroup.kill` is rejected for a threaded cgroup because killing is
process-directed
([Linux kernel: cgroup v2 thread mode](https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html)).
It can only account Engine CPU correctly if all execution attributable to that
Engine, including relevant helpers, is pinned into the same threaded cgroup.

**Finding:** cross-platform per-run limits must be enforced above the OS:
admission/concurrency limits, Wasm/Isolate memory configuration, wall-clock
deadline, operation/byte quotas, and cooperative cancellation. Linux threaded
cgroup CPU control can be an opt-in enhancement after attribution is proven;
it cannot be the documented Moonrun guarantee.

## Recommendations derived from the evidence

The statements in this section are design recommendations, not claims about OS
behavior.

### 1. Make the shared per-Run inputs explicit first

Deepen the existing execution path with explicit inputs and separate owning
host domains:

```text
RunOptions
├── immutable Policy
├── initial env -> mutable Env
├── initial cwd/root -> Host Filesystem
├── stdin / stdout / stderr -> Host Stdio
├── control receiver -> Run Termination
├── resource budget -> owning Host domains
└── network namespace inputs -> Host Network
```

Every Moonrun-owned import should resolve these values from the current Host.
No import should fall back to host `std::env`, process cwd, or process-global
stdio after Host construction. Policy describes the world and its authority;
mutable runtime state remains with the owning domain. This is the common
implementation for both multi-service and virtual-child use cases.

### 2. Keep OS adapters below Execution Supervisor

The execution Seam should eventually distinguish:

```text
Execution Adapter
├── Native Process Adapter (OS PID/Handle, native wait/signal/cleanup)
└── Engine Run Adapter     (Engine completion and virtual cancellation)
```

Execution Supervisor retains status and outcomes above both Adapters. Host
Process maps guest-visible child IDs and Handles to an `ExecutionRef` before
any platform call. This prevents a virtual ID from reaching `kill(2)`,
`waitpid()`, or `OpenProcess()` and gives both Adapters one place to implement
start, wait, stop, and destroy mechanics.

Do not start with this mapping. It is the most special part of the second use
case and depends on the shared per-Run inputs, completion, and termination
model.

### 3. Keep discovery downstream of execution ownership

Use one discovery registry populated from observed executions:

```text
workload name -> current healthy endpoints or request routes
```

The Deployment Controller owns desired workload state. Execution Supervisor
owns concrete attempts. Discovery only reflects current endpoints. A future
direct in-process request Adapter may optimize transport while preserving
caller-owned request, cancellation, and failure semantics.

### 4. Reuse the existing Completion Queue

Both a request response and an Engine child exit can be host completions.
Publish their Job identifiers to the existing Completion Queue and wake the
existing platform completion source. Do not add a second future/event-loop API
or expose `eventfd`, kqueue identifiers, or Windows Handles to guest code.

### 5. Treat native OS isolation as adapter-specific hardening

For intentionally real children:

- Linux may use pidfds for stable identity, cgroup v2 for cleanup/limits,
  Landlock for unprivileged restriction, and namespaces when deployment
  privileges permit;
- Windows should use an explicit inherited-handle list and may use a Job Object
  and AppContainer when native sandboxing is required; and
- macOS should use spawn file actions, kqueue process events, process groups,
  and rlimits, without claiming namespace-equivalent containment.

These mechanisms strengthen native children but must not change the portable
observable semantics of Engine children.

## Agile delivery sequence

Each increment has a usable done condition and proceeds from shared mechanisms
to the special process-table mapping.

### Increment 1: explicit env

Done when two concurrent Engine runs can observe different environments and no
Moonrun-owned env import reads or mutates the host process environment.

Why first: it is entirely in-memory, shared by both use cases, and exposes the
required per-run plumbing with little OS surface.

### Increment 2: virtual cwd and Host Filesystem path base

Done when two concurrent runs resolve the same relative guest path under
different virtual cwd/root values without calling process `chdir`.

Use portable dir-relative operations; optionally harden Linux opens with
`openat2()`. Do not broaden this increment into a complete WASI redesign.

### Increment 3: injected stdio and cancellation

Done when concurrent runs can independently inherit, pipe, or capture all three
streams and one run can be terminated without terminating the other or the
embedding process.

This produces the first generally useful concurrent Engine-run substrate.

### Increment 4: Execution Supervisor and Deployment Controller MVP

Done when Execution Supervisor owns threads and retained attempt outcomes, and
an external Deployment Controller converges at least two named workloads to
one local Run each with independent env/cwd/stdio/Policy. Changing desired
state stops one workload without terminating the embedding process or another
workload.

Defer distributed placement, service discovery, callable bindings, and hard
per-workload OS resource limits. They do not establish the core ownership
model.

### Increment 5: virtual `moonx` child MVP

Done when an allowlisted Wasm-target `moonx` spawn:

- returns immediately with a virtual child ID and process Handle;
- runs on a separate Engine execution unit with derived per-Run inputs;
- supports stdin/stdout/stderr, wait, exit status, and terminate;
- completes waits through the existing Completion Queue; and
- never passes its virtual ID to an OS process API.

Limit the MVP to direct, non-detached children and no silent fallback from a
denied/unsupported virtual `moonx` command to ambient OS execution.

### Increment 6: lifecycle depth and resource controls

Add virtual process groups, descendant cleanup, orphan/detach rules, deadlines,
memory/concurrency quotas, and mixed native/Engine child tests. Evaluate Linux
threaded cgroup CPU control only after all Engine-attributable threads can be
identified; add native cgroup/Job Object/AppContainer hardening independently.

## Risks and unanswered questions

- Whether all host filesystem calls have a dir-relative form or need a common
  safe path resolver, especially on Windows and macOS.
- Whether Engine execution and all per-run Host Jobs can be attributed to a
  stable set of threads; this determines whether Linux threaded cgroups can
  provide meaningful CPU accounting.
- The exact child-policy derivation rule. The safe invariant is
  `child policy <= parent policy`; Deployment-owned Policy composition needs an
  explicit, separate rule.
- Stdio backpressure and close ordering when a parent exits before a virtual
  child or service call.
- Which native-shaped process ABI observations beyond spawn/wait/terminate are
  required by MoonBit programs, especially process groups, signal numbers,
  detached execution, and handle-close/reap ordering.
- Whether a later direct in-process request route targets a warm Run or creates
  a fresh Run. This affects state isolation and performance but not the OS
  boundary found by this research.

## Conclusion

The OS should remain the implementation of **native child processes**, and its
notification/I/O primitives should remain useful backing objects. It cannot be
the implementation of **in-process virtual processes or services**. The
portable contract must therefore live in Moonrun's per-run Host state.

The highest-leverage order is:

```text
env -> cwd/filesystem base -> stdio/cancellation
    -> Execution Supervisor -> Deployment Controller
    -> virtual child ID/Handle table
    -> discovery and transport
    -> deep tree semantics and hard limits
```

That order delivers the shared execution substrate before the least reusable
part: mapping an Engine child into the native-shaped process table.
