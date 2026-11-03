# IPC execution sessions

## Scope

The supported scope is one execution session within the initiating moonrun's
native process tree. Descendants discover it only through inherited routing
metadata. There is no user-wide registry, cross-terminal discovery, automatic
background-host startup, or host that remains available after the owning Run
ends. A persistent daemon is outside the current plan, not an implied next stage.

The logical delegation tree follows Run ownership. OS parentage follows the
process actually performing spawn: a native child launched by delegated Wasm
is a child of the execution host, not of the waiting moonrun client. All normal
participants remain under the initiating host, but the two trees are not
identical. Nor does scoped discovery prove ancestry: an endpoint is a capability,
and arbitrary same-user native code can copy it. The implementation does not
claim an OS ancestry sandbox or native-child-tree supervision.

## Usable path

For a script or registry package, `moonx --daemon script.mbtx` or
`moonx --daemon user/module/package` enables a session for that execution.
The name does not imply a persistent daemon. This explicit execution option
takes precedence over the policy file's `process.ipc` setting, but grants no
permissions; selected and inherited policies still apply. Moon forwards it
through the public `moonrun --daemon` option. An embedding Engine parses the
same frontend option before artifact preparation and selects `RunOptions::with_ipc(true)`
itself. An inherited endpoint is reused instead of starting an independent host.

For an existing Wasm file, use `moonrun --daemon program.wasm` directly. This
option shares moonrun's canonical library-owned CLI grammar with future internal
frontends. It conflicts with `--virtual-process`; the effective combination is
also rejected when selected through policy or inherited IPC discovery. Parsing
does not read policy files, connect to IPC, or initialize an Engine.

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
`process.virtualize` choice. The MVP supports Linux, macOS and Windows with V8 and native process
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

Each Runtime owns an `IpcServer` after resolving its effective policy. The
platform transport owns its endpoint resources: a private socket directory on
Unix, and named-pipe handles on Windows. HostProcess adds that endpoint to native child environments,
including explicitly supplied child environments; it is reserved routing
metadata, not an ordinary user environment default. It never changes the
process-global environment. Clients remove the metadata from the supplied guest
Env, and their new Runtime gives subsequent descendants its own endpoint.

The endpoint is a local execution capability. On Unix, directory permissions restrict access to the host user. On Windows,
a local-only named pipe has an explicit DACL granting the host user access. The
random endpoint name is passed to descendants. The listener
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
options are explicit. Stdio is bound per stream: files and pipes are transferred as owned descriptors,
preserving their identity and status flags; ordinary shared Windows console
streams use the corresponding host standard stream. IPC-enabled roots and delegated Runs have
owned cwd/env, so guest mutations do not affect concurrently executing Runs.
Guest `getpid` reports the execution host; native children are OS children of
that host, even though the IPC client's PID remains visible to its own parent.
An explicitly requested stack size must match the host Engine configuration;
V8 stack flags are process-wide. Omitted stack size uses the host configuration.

## Signals, completion and teardown

On Unix the client forwards SIGINT, SIGTERM and SIGHUP to its Run; on Windows it
forwards CTRL_C and CTRL_BREAK console events. The host first tries
the existing cooperative signal channel; if the guest does not accept the
signal, it requests forced Run termination. The CLI presents a signalled outcome
using native signal termination after the final reply. Dropping or killing the
client closes its socket and requests Exit(1) for that Run. Completion is only
reported after its Runtime teardown. A lost reply or host crash leaves the
outcome unknown; no execution retry occurs.

Runtime teardown closes listener admission, shuts down both halves of client
connections (including blocked reply writes), and joins every delegated execution
before releasing the endpoint resources. Final replies additionally have a total
two-second write deadline, including partial writes, so a non-reading peer cannot
retain a worker after execution. Clients still waiting during owner shutdown
can observe a lost connection and an unknown outcome.
This recursively closes descendant sessions. A private wake socket on Unix or
cancellation event on Windows wakes admission even if the public backlog is full. Each listener bounds active
connections to 128 and applies a 30-second inactivity timeout to handshake reads;
this is not an absolute deadline for a continuously transmitting peer. Finished threads are
reaped during the owner's lifetime. An IPC host CLI treats SIGINT/SIGTERM/SIGHUP
(or Windows CTRL_C/CTRL_BREAK) as session shutdown: it closes Engine execution
admission and requests termination of every registered Run, including a root
that accepts cooperative signals. Repeated signals cannot bypass cleanup or
replace its first exit reason. Client signals still control only their associated
Run. These routes may race when the OS signals a whole foreground process group;
IPC does not broadcast an extra guest signal or promise guest cleanup handlers
will finish.

`CliSession` selects an explicit `HostSignalPolicy`; enabling IPC in a library
does not select that CLI policy. `ClientRunControl` only sends remote requests,
which the host translates to the existing signal and termination channels.
Its successful send does not acknowledge host acceptance or Run completion.

IPC-enabled caller-thread Runs register in the existing Engine process table
for host control and remove themselves after teardown. This registration does
not virtualize guest PIDs or native child spawning. Embedding hosts install their
own OS signal policy and may use per-Run control or Engine::request_shutdown;
ordinary native caller-thread Runs without IPC remain outside this table.

Forced host termination (Unix SIGKILL or Windows TerminateProcess) cannot perform
graceful cleanup; clients observe connection loss. Windows console close, logoff
and shutdown events retain native process termination behavior: returning from
those handlers does not permit asynchronous draining, unlike CTRL_C/CTRL_BREAK.

IPC sessions do not terminate arbitrary native process trees. Their ownership
covers delegated Wasm Runs; native-child supervision remains separate (#2425).
Synchronous host I/O can still delay cancellation and teardown. This is the
existing runtime limitation, not a completion acknowledgement.

## Terminals and platform details

On Unix, normal terminal input/output works when both the client and execution host are
in that terminal's foreground process group. Both ends check this before binding
terminal descriptors. This covers the ordinary inherited foreground group and
is tested with a real pseudo-terminal. IPC does not move terminal foreground
ownership; background-job and separate-process-group terminal transfers fail
explicitly. Files and pipes do not have that restriction.

On Windows, overlapped named-pipe I/O uses private cancellation events. Shutdown
wakes pending reads/writes and waits for cancellation to complete before releasing
buffers or handles. A client disappearing during admission only rejects that
connection. The server obtains the peer PID from the pipe, pins its process handle,
and duplicates file/pipe stdio into non-inheritable owned handles. The client keeps
its originals alive until acknowledgement; foreign numeric handles are never
adopted as local owners. Pending handshakes and final replies have the same bounds
as Unix. No new runtime or IPC dependency is needed beyond existing windows-sys.

Windows console handles cannot be duplicated across processes. The MVP supports
ordinary inherited console streams: the server verifies shared console membership
and that its corresponding standard stream is still a console. That stream then
uses existing host stdio, including Rust's synchronous Unicode handling and the
existing async console Jobs. Each redirected stream keeps its transferred file
or pipe, so stdout redirection can coexist with shared console stdin/stderr.
No console is opened or attached, and no new encoding adapter or forwarding pump
is introduced. Console code pages and process-global standard handles are unchanged.

A different console, or a client console whose corresponding host stream is
redirected, is rejected before execution. Custom screen buffers and console
rebinding are outside this inheritance contract; shared-console membership does
not establish arbitrary screen-buffer identity. Embedding hosts must keep their
shared standard streams stable during execution. Unix terminal foreground-group
restrictions above remain unchanged.

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

The private wire module owns bounded, versioned framing; platform transports own
connection admission, resource transfer and cancellable I/O. Frames
are limited to 1 MiB before receiver allocation. Invalid framing, versions,
initial descriptor counts fail before starting a Run. Invalid controls cancel
that connection's Run. Neither case retries execution.
The server module owns discovery, admission and the Run's immutable ceiling;
`serve` binds one invocation to the existing Engine and termination/signal
channels. Client state is decoded once. Process policy authorization stays in
HostProcess, and actual execution stays in Engine.

CLI integration tests invoke the real moonrun through a native shell and check
host/client PID separation, cwd/env/arguments, exit codes, nested permission
ceilings, client SIGTERM/SIGKILL, host shutdown and endpoint cleanup, foreground
terminal input/output, disabled IPC and stale discovery. Windows CLI tests use cmd.exe and exercise nested policy restrictions, client
CTRL_BREAK and forced death, and disabled discovery. Windows transport tests cover
early disconnects, pending-I/O cancellation, unread replies, active Run teardown,
owned handle transfer, non-UTF-8 paths and mixed console/file bindings under legacy
code pages. Host CLI shutdown tests include PID and process-group signals, repeated
signals, and a root whose cooperative cancellation handler never completes. Lower-level Unix tests cover concurrent cancellation, pipe stdio, malformed
input and resource cleanup.

Virtual-session integration and general terminal job control remain separate
work. Persistent daemons and cross-process-tree host sharing are outside the
current scope. This MVP is a session served by its existing execution host.
