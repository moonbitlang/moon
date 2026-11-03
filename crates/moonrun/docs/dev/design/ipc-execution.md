# IPC execution sessions

## Usable path

Enable a session in the existing policy file, then invoke the normal CLI:

```toml
[env]
from_host = ["*"]
[fs]
read = ["*"]
[process]
ipc = true
spawn = true
```

```sh
moonrun --policy session.toml parent.wasm
```

This example permits all native child launches; production configurations can
use `process.allow` and narrower filesystem/network rules as usual. IPC does
not grant permission. `process.ipc` defaults to false, independently of the
`process.virtualize` choice. The MVP supports Unix with V8 and native process
routing (`virtualize=false`). Unsupported combinations fail explicitly.

When parent.wasm launches an ordinary native program such as a shell, that
program inherits `MOONRUN_IPC_ENDPOINT`. If it later invokes the ordinary
`moonrun child.wasm ...`, the client connects before constructing an Engine or
initializing V8. The same host Engine executes the child on a separate thread.
The client retains a real OS PID and returns the execution outcome to its parent.
No wrapper executable or manual socket setup is required. Without an inherited
endpoint or IPC enabled in configuration, the existing local path is unchanged.
`--help` and `--version` still run locally. A stale inherited endpoint is an error;
there is no automatic local retry after an IPC request might have executed.

The Rust embedding interface selects the same behavior through
`RunOptions::with_ipc(true)`. Known moonx/moonrun interception can independently
use the same Engine; transport placement is not a guest PID policy.

## Ownership and authority

Each Runtime creates a private session directory and listener after resolving
its effective policy. HostProcess adds that endpoint to native child environments,
including explicitly supplied child environments; it is reserved routing
metadata, not an ordinary user environment default. It never changes the
process-global environment. Clients remove the metadata from the supplied guest
Env, and their new Runtime gives subsequent descendants its own endpoint.

The endpoint is a local execution capability. Directory permissions restrict
access to the host user; its random path is passed to descendants. The listener
binds the owning Run's immutable permission ceiling. Client-side selected policy
and inherited transfer add restrictions by intersection. A client cannot replace
the host ceiling by omitting a transfer or sending a more permissive one. A
nested Run receives a new endpoint bound to its narrowed authority, not its
ancestor's endpoint. Native children still have the host user's native authority,
as already documented for process spawning; IPC is not an OS sandbox against
arbitrary native code running as that user.

The client provisions its selected policy environment and resolves policy paths
before transmission. The host does not replay the parent's Env provisioning.
Program, cwd, environment, arguments, test-driver arguments and diagnostic
options are explicit. Stdio is transferred as owned descriptors, preserving
file/pipe identity and status flags. IPC-enabled roots and delegated Runs have
owned cwd/env, so guest mutations do not affect concurrently executing Runs.
Guest `getpid` reports the execution host; native children are OS children of
that host, even though the IPC client's PID remains visible to its own parent.
An explicitly requested stack size must match the host Engine configuration;
V8 stack flags are process-wide. Omitted stack size uses the host configuration.

## Signals, completion and teardown

The client forwards SIGINT, SIGTERM and SIGHUP to its Run. The host first tries
the existing cooperative signal channel; if the guest does not accept the
signal, it requests forced Run termination. The CLI presents a signalled outcome
using native signal termination after the final reply. Dropping or killing the
client closes its socket and requests Exit(1) for that Run. Completion is only
reported after its Runtime teardown. A lost reply or host crash leaves the
outcome unknown; no execution retry occurs.

Runtime teardown closes listener admission, shuts down both halves of client
connections (including blocked reply writes), and joins every delegated execution
before removing the endpoint directory. Final replies additionally have a total
two-second write deadline, including partial writes, so a non-reading peer cannot
retain a worker after execution. Clients still waiting during owner shutdown
can observe a lost connection and an unknown outcome.
This recursively closes descendant sessions. A private wake socket wakes the
listener even if the public backlog is full. Each listener bounds active
connections to 128 and applies a 30-second inactivity timeout to handshake reads;
this is not an absolute deadline for a continuously transmitting peer. Finished threads are
reaped during the owner's lifetime. Root CLI signals use Run termination so that
repeated signals cannot bypass this cleanup. SIGKILL of the host cannot perform
graceful cleanup; clients observe connection loss.

IPC sessions do not terminate arbitrary native process trees. Their ownership
covers delegated Wasm Runs; native-child supervision remains separate (#2425).
Synchronous host I/O can still delay cancellation and teardown. This is the
existing runtime limitation, not a completion acknowledgement.

## Terminals and platform details

Normal terminal input/output works when both the client and execution host are
in that terminal's foreground process group. Both ends check this before binding
terminal descriptors. This covers the ordinary inherited foreground group and
is tested with a real pseudo-terminal. IPC does not move terminal foreground
ownership; background-job and separate-process-group terminal transfers fail
explicitly. Files and pipes do not have that restriction.

Linux receives descriptors atomically with CLOEXEC. Darwin requires a later
fcntl, leaving a receive-to-fcntl inheritance window during concurrent native
spawn. A spawn can retain another Run's pipe ends; strict inheritance isolation
requires coordination with all native process creation. The adapter does not
claim that guarantee. It receives up to 512 descriptors before validating the
required count of three, covering the Linux and XNU kernel limits and avoiding
Darwin's truncated-ancillary descriptor leak. rustix 1.1.6 additionally contains
the upstream bounds-checking fix for truncated control headers. The larger
receive buffer remains necessary for the kernel behavior.

## Implementation and validation

The private wire module owns bounded, versioned framing and fd transfer. Frames
are limited to 1 MiB before receiver allocation. Invalid framing, versions,
initial descriptor counts fail before starting a Run. Invalid controls cancel
that connection's Run. Neither case retries execution.
The session module owns discovery, admission and the Run's immutable ceiling;
`serve` binds one invocation to the existing Engine and termination/signal
channels. Client state is decoded once. Process policy authorization stays in
HostProcess, and actual execution stays in Engine.

CLI integration tests invoke the real moonrun through a native shell and check
host/client PID separation, cwd/env/arguments, exit codes, nested permission
ceilings, client SIGTERM/SIGKILL, host shutdown and endpoint cleanup, foreground
terminal input/output, disabled IPC and stale discovery. Lower-level tests cover
concurrent cancellation, pipe stdio, malformed input and resource cleanup.

A machine-wide persistent daemon, automatic daemon startup, Windows transport,
virtual-session integration and general terminal job control remain follow-ups.
This MVP is a session served by an existing execution host, not a system service.
