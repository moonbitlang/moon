# Moon CLI execution lifecycle

## Status

Accepted and implemented.

This design owns top-level invocation selection, Moon command execution,
tracing finalization, and final process actions. The
[`moon run` process lifecycle](moon-run-process-lifecycle.md) remains the
authority for the child process launched by `moon run`.

## Problem

The CLI accepts more than one argument grammar:

- Clap parses ordinary Moon commands.
- The executable name selects `moonx`.
- generated build commands use the hidden `moon tool exec` parser.
- compatibility forms such as external `cram` commands and `help ide` delegate
  an opaque argument tail.

Previously these cases were discovered at different points in `main`, and some
helpers received a Clap error only to decide whether they should run. Command
execution, output construction, tracing, workspace bootstrap, delegation, and
process exit were interleaved. A successful Unix delegation could therefore
replace Moon before tracing guards were dropped.

## Invocation selection

`cli/invocation.rs` owns all argument interpretation. Its public operation is
conceptually:

```rust
fn select(raw_args: Vec<OsString>)
    -> Result<SelectedInvocation, clap::Error>;
```

Selection produces one of four outcomes:

```rust
enum SelectedInvocation {
    Help,
    Moon(Box<MoonInvocation>),
    Moonx(MoonxInvocation),
    Delegate(DelegatedInvocation),
}
```

It does not print, change directory, initialize output or tracing, inspect
workspace state, or start a child process.

The selector owns the interaction between Clap and compatibility grammar. It
first selects executable-name and hidden-tool parsers, then parses ordinary
Moon commands. The private prefix parser is used only where an opaque delegate
tail prevents Clap from parsing the complete invocation. Cram and external
modules receive an already selected invocation; they do not inspect raw
arguments or Clap errors.

### Argument ownership at delegation points

A global option before a transparent delegation point belongs to Moon. An
option after that point belongs to the delegated executable. For example:

- `moon --trace cram --version` asks Moon to trace a transparent delegate and
  is rejected during selection.
- `moon cram --trace` forwards `--trace` to `moon-cram`.
- `moon cram test --trace` selects the Moon-owned `cram test` wrapper, so the
  option belongs to Moon.

`cram --help` remains Clap-rendered help for the Moon wrapper. Other cram
parent flags and non-`test` subcommands belong to `moon-cram`.

Clap help and parse failures retain Clap's native rendering and exit behavior.
They occur before a Moon command runtime exists.

## Moon command and output selection

A selected Moon invocation contains its parsed command, universal flags, and
an orthogonal output format:

```rust
enum OutputFormat {
    Human,
    Json,
}
```

`check` is currently the only command that selects JSON. The distinction is an
output contract, not a `JsonCheck` command variant. A future JSON command may
own a different result schema while reusing the same lifecycle requirement:
one complete Command Result on stdout and no terminal-only output on stderr.
No universal JSON payload or generic result trait is required.

## Runtime

`cli/runtime.rs` is the only consumer of `SelectedInvocation`.

Transparent delegates skip Command Output, workspace bootstrap, and Moon
tracing. Moon and moonx invocations construct only the state their selected
execution path owns.

For an ordinary Moon command, the runtime performs these steps:

1. construct the selected Command Output and User Log;
2. apply the selected working directory;
3. initialize tracing;
4. load workspace environment and emit command warnings;
5. dispatch the parsed command and receive a `ProcessAction`;
6. drop the tracing guard;
7. execute the process action; and
8. return one integer exit code to `main`.

Unhandled human-command errors have one User Log renderer. JSON errors after
selection are converted into the command's JSON result and serialized once.

## Process actions

Command handlers do not terminate or replace Moon. They may request one final
action:

```rust
enum ProcessAction {
    Exit(i32),
    Delegate(Command),
}
```

Most commands return `Exit`. A command such as registry-backed `runwasm` or
`cram test` may perform Moon-owned preparation and return a fully configured
child command as `Delegate`.

The runtime executes the action only after tracing is finalized. On Unix,
delegation may then replace the Moon process. On Windows, Moon waits for the
child while preserving the existing Ctrl-C behavior. Transparent delegates
use the same final executor without creating a Moon command runtime first.

`main.rs` owns only panic-hook setup, raw argument collection, one runtime
call, and the final `process::exit`. The retained exception is Clap's native
parse-error exit before managed command execution begins.

## Tracing ownership

Tracing follows work ownership rather than command names:

- transparent delegation does not initialize Moon tracing;
- a Moon-owned command traces its Moon-owned work;
- a command that prepares and then delegates finalizes its trace before the
  delegated program starts; and
- `moon run` retains its separately documented spawn/wait lifecycle.

Moon does not claim to trace work inside a delegated executable. Cross-process
tracing would require an explicit propagation and collection protocol.

## Process categories kept separate

This lifecycle does not merge the communication categories defined by
[ADR 0004](../../adr/0004-separate-command-results-from-user-logs.md).
Command Results, User Logs, Process Passthrough, Progress Displays, tracing,
and Moon-managed setup-child output remain separate mechanisms.

## Moon-managed setup children

Moon-managed setup children currently include legacy `postadd` hooks and the
nested Moon process used for deprecated binary dependencies. Their output mode
is selected explicitly at the command seam:

```rust
enum ChildOutputMode {
    Inherit,
    Capture,
}
```

Human commands inherit the child's stdout and stderr. JSON check captures both
channels, closes the child's stdin, and waits for completion before producing
the Command Result. Non-empty output from a successful child becomes an
informational User Log entry; output from a failed child becomes an error.
User Log filtering and child-output mode remain independent decisions: capture
is never inferred from the User Log destination.

Registry adapters only acquire and verify source. Project-local dependency
installation invokes the isolated legacy-postadd module after materialization,
while immutable cached sources reject postadd and never receive process-output
policy. Package prebuild tasks, including custom `pre-build`, moonlex, and
moonyacc, remain build-graph work owned by the build executor. An unstable
module prebuild script run by a nested binary-dependency build is covered by
the output mode of that nested Moon process as a whole. User programs remain
Process Passthrough.

## Regression coverage

Selection tests use raw argument vectors and cover ordinary commands, JSON
selection, executable-name dispatch, tool exec, IDE help, cram ownership, and
trace placement.

Managed-child runner tests cover inherited and captured output on both output
channels, stdin EOF, and success/failure classification.

Public CLI tests cover:

- external and cram delegation with effective `-C` resolution;
- transparent delegate signal handling;
- trace ownership around the cram delegation point;
- complete traces before `cram test` and registry `runwasm` delegation;
- complete JSON stdout and trace files for successful and failed JSON checks;
- JSON check capture for successful and failed legacy postadd hooks and nested
  binary-dependency builds; and
- existing `moon run` trace and temporary-project cleanup invariants.
