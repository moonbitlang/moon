# `moon run` process lifecycle

## Status

Accepted and implemented.

This decision applies to the process that executes a program for `moon run`.
It does not require `moon test` to stop using its asynchronous runner, because
tests have different requirements for output capture, cancellation, and
parallel execution.

## Context

`moon run` used to pass the requested program through the shared Tokio child
runner. On macOS, that runner temporarily blocks `SIGCHLD` during process
creation to work around a suspected Tokio child-wait race.

Signal masks are inherited at process creation. Blocking `SIGCHLD` around
spawn therefore also started the requested user program with `SIGCHLD`
blocked. A program that later spawned and waited for its own children could
hang with zombie processes. Build tools exposed this particularly clearly:
queries that did not create children completed, while compiler detection and
other subprocess-heavy operations could hang.

The upstream report involved Tokio 1.39.2, which is also the version originally
used by Moon. The proposed Tokio change to block `SIGCHLD` during spawn was not
merged. Tokio maintainers noted that changing a thread's signal mask around
spawn is unsafe in the presence of concurrent spawns and other signal users,
and the underlying race was never confirmed.

Unlike the test runner, `moon run` does not need asynchronous output capture or
parallel child management. It is a synchronous operation: build one artifact,
launch it with inherited standard streams, and wait for it to finish.

## Decision

`moon run` uses `std::process::Command` and a blocking `spawn`/`wait` lifecycle
on every platform.

The execution flow is:

1. Build the selected artifact while holding the target-directory lock.
2. Construct its runtime command as `std::process::Command`.
3. Release the build lock.
4. Install the platform's parent-side terminal-signal handling.
5. Spawn the child without the Tokio process runner or the macOS `SIGCHLD`
   masking wrapper.
6. On Windows, assign the child to the existing kill-on-close Job Object.
7. Wait for the child and return its exit result to `main`.
8. Let normal Rust destruction finalize process-scoped and caller-owned state.

On Unix, termination by signal is reported as the conventional shell-visible
`128 + signal` exit status. This preserves the result users normally observe
from a shell, although an outer `waitpid` observes a normal exit with that code
rather than `WIFSIGNALED`.

On Windows, Moon's console handler keeps the parent alive while the console
delivers Ctrl-C independently to the child. The Job Object has a separate
responsibility: it cleans up the child process tree if the parent exits.

Command construction remains shared between run and test paths. The test
runner converts the standard command into a Tokio command only at its own
execution seam. On macOS, that seam retains the historical wait workaround but
restores the parent's original signal mask in the child before `exec`, so the
temporary block does not escape into the test program.

## Why Unix `exec` was rejected

An earlier implementation used `exec` on Unix. It appeared attractive because
it removed the intermediate Moon process, could not pass Moon's later signal
mask changes to another spawn, and preserved raw Unix signal status.

However, a successful `exec` never returns. It bypasses every Rust destructor
owned by Moon, including state outside the run module:

- `moon run -e` and `moon run -` own a `tempfile::TempDir` containing their
  synthesized source project and build outputs. Without a return path, those
  directories are leaked.
- `main` owns the `tracing_chrome` guard. Its destructor flushes and finalizes
  Chrome trace output. Without it, queued events can be lost and `trace.json`
  can be incomplete or invalid.
- Future process-scoped RAII state would fail in the same way even if today's
  two known resources were special-cased.

Explicitly flushing the trace guard before `exec` is the wrong seam. The run
module does not own that guard, and teaching it about selected resources would
split cleanup knowledge between `main`, callers, and process execution.
Likewise, deleting the inline-source directory before `exec` is impossible
because the executable being launched lives inside that directory.

Using `exec` only for invocations that currently appear not to own temporary
state was also rejected. Whether `exec` is safe would then depend on invisible
ownership in every caller and on future RAII additions. The short-lived
`ReplaceProcess` versus `WaitForCleanup` distinction demonstrated this
problem: it exposed a lifecycle implementation detail through the run
interface without making the classification future-proof.

## Other rejected alternatives

### Retain the Tokio runner for `moon run`

This would preserve normal destruction, but would reintroduce the original
`SIGCHLD` inheritance through Moon's macOS workaround. Removing that
workaround alone would rely on an upstream child-wait race that was reported
against the same Tokio version and was not conclusively fixed.

Tokio remains appropriate for `moon test`, where asynchronous capture and
parallel child execution provide real value. Those requirements do not apply
to `moon run`.

### Add defensive child-side signal-mask changes

Unconditionally unblocking `SIGCHLD` in the child can conceal a bug in the
parent runner and silently changes the signal-mask contract inherited from the
caller. `moon run` has no need to manipulate the mask at all once it avoids the
masking runner. Defensive normalization in a general process library would be
a separate decision with separate compatibility requirements. This differs
from the test runner restoring the exact mask that existed before its own
temporary block.

### Use a cleanup helper process

A helper could retain `exec` for the target while another process monitored
its lifetime and removed temporary state. It would also need a reliable
protocol for trace finalization, signal forwarding, process groups, abnormal
termination, and Windows Job Object behavior. That is substantially more
lifecycle machinery than a synchronous parent waiting for one child.

### Periodically poll the child

No periodic status probe is needed. Blocking `wait` is event-driven by the
operating system and lets the parent perform cleanup when the child exits.

## Required invariants

Changes to `moon run` process execution must preserve all of the following:

- The user program is not spawned through `tokio::process::Command`.
- The user program is not spawned while Moon has temporarily blocked
  `SIGCHLD`.
- Moon remains alive until the child exits so control returns through `main`
  and caller scopes.
- The build lock is released before the user program starts.
- Standard input, output, and error retain interactive command behavior.
- Ctrl-C reaches the child while the parent remains alive long enough to
  finish cleanup.
- Windows descendants remain covered by the kill-on-close Job Object.
- Inline and stdin source projects are removed after normal execution.
- Chrome trace output is flushed into valid JSON.
- Waiting does not depend on periodic polling.

An uncatchable termination such as `SIGKILL` can still bypass cleanup. That is
an operating-system limitation and not a reason to weaken cleanup on normal
exit or handled terminal interruption.

## Regression coverage

The process-lifecycle behavior is anchored by integration tests:

- `test_moon_run_command_string_removes_temporary_project` verifies that a
  successful inline run leaves its isolated temporary root empty.
- `test_moon_run_flushes_trace` parses `trace.json`, catching an unflushed or
  incomplete Chrome trace.
- `test_native_abort_trace` verifies the shell-visible signal exit status and
  ensures Moon does not add a wrapper error to the child's panic output.
- The run-command suite covers normal Wasm, JavaScript, native error, argument,
  and backtrace behavior.
- On macOS, `spawned_child_can_asynchronously_wait_for_its_child` verifies that
  the test runner does not pass its temporary `SIGCHLD` block into a child
  which asynchronously waits for another process.

Prefer end-to-end child behavior over signal-mask probes when adding
regressions. A representative program that spawns and waits for its own child
tests the user-visible contract without coupling the test to mask inspection.

## Reconsidering this decision

Do not replace the standard spawn/wait path with `exec` solely to remove one
parent process or recover raw `WIFSIGNALED` status.

Reconsideration requires a concrete requirement that cannot be met by the
current lifecycle and a design that:

- identifies one owner for all process-scoped finalization;
- preserves temporary-project and tracing cleanup without enumerating
  resources in the run module;
- specifies Ctrl-C, direct termination, descendant, and Windows Job Object
  behavior;
- avoids periodic polling and unsafe signal-mask changes around concurrent
  spawn;
- includes end-to-end regressions for nested child creation, trace validity,
  temporary cleanup, and signal exit behavior; and
- updates this decision record with the new trade-off.

If a future Tokio release conclusively fixes the macOS child-wait problem, that
may justify simplifying the asynchronous test runner. It does not by itself
justify moving the synchronous `moon run` path back to Tokio or `exec`.

## References

- [Tokio issue #6770: `Command.wait` hanging on macOS](https://github.com/tokio-rs/tokio/issues/6770)
- [Unmerged Tokio PR #6953: block `SIGCHLD` while spawning](https://github.com/tokio-rs/tokio/pull/6953)
- [Microsoft `SetConsoleCtrlHandler` documentation](https://learn.microsoft.com/en-us/windows/console/setconsolectrlhandler)
- [Microsoft Job Objects documentation](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)
