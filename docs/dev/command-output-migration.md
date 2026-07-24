# Command Output Migration

Status: accepted; CO-1, CO-2, CO-4, and CO-5 complete; CO-7 in progress

## Goal

Every MoonBuild-authored Command Result and User Log has one explicit owner and follows the same channel, filtering, formatting, and error policy, while Process Passthrough, Progress Displays, and tracing remain explicit separate mechanisms.

The migration must preserve observable command behavior unless a slice names and tests an intentional change. Each slice should be independently reviewable and leave the tree ready for the next slice.

Splitting stdout from stderr is independent from normalizing default verbosity. The shared CLI mapping is error-only under `--quiet`, informational by default, and debug under `--verbose`. Commands such as `moonx` may retain an explicitly quieter default when transparency is part of their contract.

## Interface Shape

`moonutil` should expose a `CommandOutput` facade constructed from the command's
user-log level and quiet policy. Its intentionally small interface is:

- borrow its `UserLog` for filtered stderr messages;
- run a closure against locked stdout to render a fallible Command Result;
- render an informational stdout status unless quiet output was requested.

The stdout operation should preserve the renderer's result type, propagate its `std::io::Write` failures, and hold the lock for the entire logical result. It should not grow methods for every output format.

Both standard-stream boundaries are adaptive. Renderers emit ANSI styles, and the stdout or stderr writer retains or strips them according to that destination's terminal and environment policy. Color detection must not consult a different global stream: stdout and stderr may be redirected independently.

Renderers below the CLI seam should accept a writer. Library operations should return data when the CLI owns presentation. A lower-level operation may accept `UserLog` when emitting a user-facing event is inherently part of that operation, but pure build planning should not depend on `CommandOutput`.

The facade does not own Process Passthrough, Progress Displays, compiler diagnostics, or tracing. Those paths have different byte-preservation, concurrency, terminal, and configuration contracts and should be migrated only within their own seams.

## Behavioral Policy

| Communication | Channel | Filtered | Write policy | Initial owner |
| --- | --- | --- | --- | --- |
| Command Result | stdout | never | return failures | `CommandOutput` |
| Command Status | stdout | `--quiet` | return failures | `CommandOutput` |
| User Log | stderr | by user-log level | best effort, matching `UserLog` | `UserLog` |
| Compiler diagnostics | renderer-selected | diagnostic policy | renderer-specific | diagnostic renderer |
| Process Passthrough | original child channel | never | preserve bytes and ordering as far as the executor permits | run/build executor |
| Progress Display | terminal stderr | quiet-aware | renderer-specific lifecycle | progress renderer |
| tracing | configured tracing sink | `RUST_LOG`/trace configuration | tracing subscriber policy | tracing setup |

`UserLog` is the sole owner of filtered user-facing stderr. Informational and debug messages are rendered as bare lines, while warnings and errors retain their canonical labels.

The quiet user-log level follows `UserLog` and suppresses warnings. Explicit inputs that are skipped while other work continues are warnings. Automatic workspace filtering is silent; the existing behavior when all explicit inputs are skipped remains a separate command-semantics concern.

## Slices and Blocking Edges

### CO-1: Prove the seam with `moon info`

Status: complete

Blocks: CO-2, CO-3

- Add `moonutil::CommandOutput` composed with `UserLog`.
- Route the existing `moon info` backend-difference report through one locked stdout writer closure.
- Make `UserDiagnostics` a compatibility adapter over `UserLog`, without changing which commands default to quiet.
- Make ANSI styling follow the actual stdout or stderr destination instead of global stdout detection.
- Keep build execution output on its current paths.
- Test the public `moon` process seam: backend differences remain on stdout under `--quiet`, while a known warning is filtered on stderr.
- Unit-test `UserLog` filtering and destination-controlled ANSI rendering.
- Done when `moon info` contains no direct stdout macro or handle access and its existing output remains compatible.

### CO-2: Establish command-level construction

Status: complete

Blocked by: CO-1

Blocks: CO-4, CO-5

- Construct one `CommandOutput` after universal flags and workspace setup are known.
- Pass references through command dispatch so migrated commands can receive the command-owned log without reconstructing its policy.
- Keep early argument/help and pre-dispatch failures on explicit bootstrap output paths.
- Done when commands can receive the output context without reconstructing output policy.

### CO-3: Migrate self-contained result commands

Blocked by: CO-1

Blocks: CO-6

- Migrate one command family per change: `version`, `whoami`, `explain`, shell completion, and other leaf commands.
- Keep machine-readable output byte-for-byte stable and add channel assertions where missing.
- Prefer a renderer taking a writer when a command emits more than one fragment.
- Done when the selected command family has no unclassified direct stdout writes.

### CO-4: Replace `UserDiagnostics`

Status: complete

Blocked by: CO-2

Blocks: CO-6, CO-7

- Pass the command-owned `UserLog` through every build-facing command family.
- Preserve each command's current default log level; pin error-only `--quiet` and the bare informational-message policy.
- Emit user-facing events directly through `UserLog`; do not buffer them in a parallel warning transport.
- Keep output policy out of build planning configuration by passing `UserLog` only to operations that emit events.
- Treat promotion of semantic warnings under `--deny-warn` as a separate policy change at the `UserLog` seam.
- Delete `UserDiagnostics` after its last caller moves; do not keep it as a permanent wrapper over `UserLog`.
- Done when `UserDiagnostics` is removed and all user-authored log sites use the shared level policy.

### CO-5: Migrate dry-run and graph Command Results

Status: complete

Blocked by: CO-2

Blocks: CO-7

- Route dry-run commands, graph exports, and other build reports through writer-based renderers.
- Preserve which reports intentionally use stdout versus stderr.
- Keep planning data independent from CLI output types.
- Render directly composed graph-plus-command previews, such as `run` and
  `cram`, under one stdout lock.
- Keep the test-only graph dump as an explicit file side effect after successful
  dry-run rendering.
- Done when Rupes Recta and legacy dry-run output share the Command Result policy without sharing renderer implementation unnecessarily.

### CO-6: Move package-manager presentation to the CLI seam

Blocked by: CO-3, CO-4

- Replace `mooncake` direct result printing with returned report data or writer-based rendering.
- Pass `UserLog` only to operations whose user-facing events occur during long-running work such as downloads.
- Migrate add, fetch, tree, work, update, install, and registry notifications in small command-family changes.
- Done when `mooncake` has no unclassified direct stdout or stderr writes.

### CO-7: Classify build execution output

Status: in progress

Blocked by: CO-4, CO-5

Blocks: CO-8

- Separate durable build summaries from Progress Displays and Process Passthrough.
- Keep build execution independent from durable result presentation: it emits
  diagnostics and progress through their existing seams and returns build
  statistics to its caller.
- Write human `Finished ...` and `Failed with ...` build results to stdout.
  Quiet output suppresses successful status, while failed results remain
  visible. Write failure context through `UserLog::error`; compiler
  diagnostics remain independently owned.
- Treat a closed stdout pipe as the consumer finishing early, without changing
  the semantic build exit status. Propagate other stdout write failures.
- Omit the build result when a command owns a subsequent primary result, such
  as running a program, reporting proof results, or running tests.
- Normalize the shared CLI user-log mapping after reclassifying command echoes
  and cache details as debug-only messages.
- Migrate build reports to writer-based Command Results.
- Keep child stdout/stderr forwarding byte-preserving and keep concurrent progress behind its own renderer seam.
- Cover both Rupes Recta and the legacy engine in each applicable behavior test.
- Done when every executor print site is either migrated or documented as passthrough/progress.

### CO-8: Close the inventory

Blocked by: CO-6, CO-7

- Classify remaining production print macros and direct standard-stream handles.
- Add a lightweight repository check or documented allowlist only after the categories are stable.
- Update developer reference documentation with the final ownership rules and remove this migration roadmap.
- Done when new unclassified MoonBuild-authored output is difficult to introduce accidentally.

## Review and Verification Per Slice

Use the public `moon` process as the primary test seam and assert stdout and stderr separately. Add renderer tests only when formatting logic has meaningful branches that are cheaper to exercise through an injected writer. Every slice should run its focused integration tests, formatting, and the affected crate checks before review; broader workspace verification belongs at milestone boundaries.
