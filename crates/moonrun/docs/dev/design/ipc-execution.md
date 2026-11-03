# IPC execution: first increment

IPC and in-process interception are two entries into the same Engine. Known
moonx/moonrun calls can eventually bypass an OS client process; an externally
started moonrun can submit execution through IPC. Unknown programs themselves
still execute normally. Neither transport selection nor guest virtualization
grants authority.

This increment provides a Unix library adapter and an executable example. It
supports one direct Wasm invocation per connected socket, with explicit caller
state, resource transfer, completion, forced termination, and disconnect
cancellation. It does not change the moonrun CLI, create a daemon automatically,
or depend on native-child supervision in #2425. Windows transport, cooperative
signal forwarding, and internal moonrun interception are follow-ups.

## Ownership and interface

The embedding host establishes and authorizes a connection, chooses restrictions,
and calls `ipc::serve(&engine, connection, options)`. Execution stays on that
calling thread. The host can schedule concurrent connections with Engine clones;
each connection owns a separate Run termination channel. IPC does not call
`Engine::request_shutdown` and never uses an OS PID as a virtual identity.

The client submits an `Invocation` and three owned stdio files, retains a
`ClientRun`, and consumes its outcome with `wait`. A cloned `RunControl` can send
forced termination while another thread waits. Sending is not acknowledgement
of acceptance or completion. Dropping ClientRun shuts down the connection even
if a control clone survives. An EOF or malformed control message requests
Exit(1) for only that Run. As with the library Run interface, synchronous host
I/O can delay cancellation; requesting cancellation is not evidence of cleanup.

Only native guest-process routing and V8 are supported in this increment.
Virtual sessions require explicit ownership and draining of descendants before
completion. They are rejected here rather than silently sharing a namespace or
leaving managed descendants behind. This restriction does not tie future IPC
placement to the `process.virtualize` configuration.

## State and authority

- Program and cwd are explicit absolute paths. Paths and environment retain
  Unix bytes, including non-UTF-8 data. Env, cwd, args and stdio never default to
  the execution host's ambient state. No process-wide chdir or env mutation is
  used. Existing selected-policy Env provisioning still applies to this Env.
- Stdio uses SCM_RIGHTS, retaining file/pipe identity, offsets and status flags.
  Received descriptors are owned and marked CLOEXEC. Linux sets CLOEXEC while
  receiving; Darwin requires a subsequent fcntl, leaving the platform's small
  receive-to-fcntl inheritance window. This is not an atomic descriptor grant
  against arbitrary concurrent exec on Darwin. An overlapping native spawn can
  retain another Run's pipe ends after its teardown. Hosts requiring atomic
  inheritance isolation must coordinate admission with all native process
  creation; this adapter does not provide that global coordination. Terminal
  stdio is rejected:
  transferring a descriptor does not transfer foreground process-group status.
- Host restrictions are supplied out of band through RunOptions. An optional
  client transfer adds restrictions by intersection, including when the host
  already supplies inherited restrictions. It cannot replace the host ceiling.
  Missing client restrictions never remove host restrictions. Malformed
  transfers fail before guest execution.
- An accepted connection is an execution capability. This adapter does not
  authenticate peers or recover a parent session's authority from client claims.
  A future discovery/session layer must bind verified parent authority before
  exposing connections to descendant processes. Guest-supplied policy bytes
  are additional constraints, not credentials. This is not a network service.

The host owns program loading, as with Engine::run_file; file contents are not
frozen at client submission. Guest getpid currently reports the execution host
PID. Native children are OS children of that host, with existing native lifetime
semantics. The client has a real PID but cannot yet claim transparent native
parentage, signal, accounting or process-group behavior for the remote Run.
Guest stdio is bound to the client, while existing host logging and some policy
diagnostics still go to the host. Full per-client diagnostic routing remains a
separate runtime concern.

## Protocol and failure

The experimental Unix protocol starts with one `R` byte carrying exactly three
stdio descriptors, followed by a versioned JSON request in a four-byte big-endian
length frame. The same framing carries controls and one final reply. The synchronous adapter
sets its exclusively owned control socket to blocking mode. Stdio file-status
flags are preserved. Every
frame is bounded to 1 MiB before receiver allocation. Unsupported versions,
malformed data, and initial descriptor sets with a count other than three fail
closed; received owners are released on every error path. Request execution
starts only after decoding and binding complete. The ancillary buffer accepts
512 descriptors before validating that exactly three arrived: this covers
Linux's 253-fd SCM_RIGHTS limit and [XNU's UIPC_MAX_CMSG_FD limit](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/uipc_usrreq.c).
This avoids Darwin's descriptor leaks and the [rustix truncation bug](https://github.com/bytecodealliance/rustix/issues/1683).
The one-byte receive consumes one rights-bearing send; protocol limits are
applied after owning the complete kernel-delivered descriptor set.

One connection executes at most once. There is no retransmission, reconnect
retry, or automatic local fallback. Host failure or a lost reply leaves the
outcome unknown; the program may have produced effects. Only an observed final
reply reports Engine completion after teardown. Host-side errors are distinct
from guest exit and signal outcomes. Endpoint permissions, admission limits,
handshake deadlines, scheduling and listener shutdown belong to the embedding
host, not to the wire decoder.

## Remaining work

1. Agree on endpoint/session admission and discovery, including connecting to an
   existing execution host versus starting a standalone daemon. Bind a verified
   session ceiling; do not trust a policy blob to identify its parent.
2. Adapt external moonrun entry to capture its explicit state and restriction
   transfer before materializing Env, forward cooperative signals, and present
   exit status. Decide client PID visibility and native child ownership.
3. Add Windows resource transfer and cancellation, terminal handling, and virtual
   session draining with the same per-execution ownership contract.
4. Integrate known moonx/moonrun interception with the common execution entry.
   Benchmark cold/warm startup before promising a performance improvement.

The tests cover cross-process execution, caller cwd/env/args, pipe stdio,
concurrent execution cancellation, disconnect with retained control handles,
permission intersection, error outcomes, byte-preserving paths, bounded framing,
and descriptor cleanup. They do not establish daemon discovery, OS signal
transparency, synchronous I/O cancellation, or crash isolation guarantees.
