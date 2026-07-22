# Command Output Migration

Status: accepted; CO-1 complete

## Goal

Every MoonBuild-authored Command Result and User Log has one explicit owner and follows the same channel, filtering, formatting, and error policy, while Process Passthrough, Progress Displays, and tracing remain explicit separate mechanisms.

The migration must preserve observable command behavior unless a slice names and tests an intentional change. Each slice should be independently reviewable and leave the tree ready for the next slice.

Splitting stdout from stderr is independent from normalizing default verbosity. The quiet user-log level is error-only, whether selected explicitly or by a command default, but commands such as `moonx` may retain a quieter default until a later slice deliberately changes it.

## Interface Shape

`moonutil` should expose a `CommandOutput` facade constructed from the command's user-log level. Its intentionally small interface is:

- borrow its `UserLog` for filtered stderr messages;
- run one closure against locked stdout to render a fallible Command Result.

The stdout operation should preserve the renderer's result type, propagate its `std::io::Write` failures, and hold the lock for the entire logical result. It should not grow methods for every output format.

Renderers below the CLI seam should accept a writer. Library operations should return data when the CLI owns presentation. A lower-level operation may accept `UserLog` when emitting a user-facing event is inherently part of that operation, but pure build planning should not depend on `CommandOutput`.

The facade does not own Process Passthrough, Progress Displays, compiler diagnostics, or tracing. Those paths have different byte-preservation, concurrency, terminal, and configuration contracts and should be migrated only within their own seams.

## Behavioral Policy

| Communication | Channel | Filtered | Write policy | Initial owner |
| --- | --- | --- | --- | --- |
| Command Result | stdout | never | return failures | `CommandOutput` |
| User Log | stderr | by user-log level | best effort, matching `UserLog` | `UserLog` |
| Process Passthrough | original child channel | never | preserve bytes and ordering as far as the executor permits | run/build executor |
| Progress Display | terminal stderr | quiet-aware | renderer-specific lifecycle | progress renderer |
| tracing | configured tracing sink | `RUST_LOG`/trace configuration | tracing subscriber policy | tracing setup |

One current behavior requires an explicit compatibility decision in its migration slice:

- `UserDiagnostics` prefixes informational messages with `Info:`, while `UserLog` renders informational messages without a label.

The quiet user-log level follows `UserLog` and suppresses warnings. Default levels and informational formatting remain command-specific until affected command snapshots make any intended behavior change visible.

## Slices and Blocking Edges

### CO-1: Prove the seam with `moon info`

Status: complete

Blocks: CO-2, CO-3

- Add `moonutil::CommandOutput` composed with `UserLog`.
- Route the existing `moon info` backend-difference report through one locked stdout writer closure.
- Apply the resolved error-only quiet invariant to the transitional `UserDiagnostics`, without changing which commands default to quiet.
- Keep build execution output on its current paths.
- Test the public `moon` process seam: backend differences remain on stdout under `--quiet`, while a known warning is filtered on stderr.
- Unit-test the transitional `UserDiagnostics` quiet invariant because it is shared by commands not yet migrated to `UserLog`.
- Done when `moon info` contains no direct stdout macro or handle access and its existing output remains compatible.

### CO-2: Establish command-level construction

Blocked by: CO-1

Blocks: CO-4, CO-5

- Construct one `CommandOutput` after universal flags and workspace setup are known.
- Pass references through command dispatch without migrating all commands in the same change.
- Keep early argument/help and pre-dispatch failures on explicit bootstrap output paths.
- Done when a command can receive one output context without reconstructing log policy in nested functions.

### CO-3: Migrate self-contained result commands

Blocked by: CO-1

Blocks: CO-6

- Migrate one command family per change: `version`, `whoami`, `explain`, shell completion, and other leaf commands.
- Keep machine-readable output byte-for-byte stable and add channel assertions where missing.
- Prefer a renderer taking a writer when a command emits more than one fragment.
- Done when the selected command family has no unclassified direct stdout writes.

### CO-4: Replace `UserDiagnostics` incrementally

Blocked by: CO-2

Blocks: CO-6, CO-7

- Move one build-facing command family at a time to `UserLog`.
- Preserve each command's current default log level unless the slice explicitly changes it; pin error-only `--quiet` and any informational-label decision for the affected family.
- Delete `UserDiagnostics` only after its last caller moves; do not keep it as a permanent wrapper over `UserLog`.
- Done when `UserDiagnostics` is removed and all user-authored log sites use the shared level policy.

### CO-5: Migrate dry-run and graph Command Results

Blocked by: CO-2

Blocks: CO-7

- Route dry-run commands, graph exports, and other build reports through writer-based renderers.
- Preserve which reports intentionally use stdout versus stderr.
- Keep planning data independent from CLI output types.
- Done when Rupes Recta and legacy dry-run output share the Command Result policy without sharing renderer implementation unnecessarily.

### CO-6: Move package-manager presentation to the CLI seam

Blocked by: CO-3, CO-4

- Replace `mooncake` direct result printing with returned report data or writer-based rendering.
- Pass `UserLog` only to operations whose user-facing events occur during long-running work such as downloads.
- Migrate add, fetch, tree, work, update, install, and registry notifications in small command-family changes.
- Done when `mooncake` has no unclassified direct stdout or stderr writes.

### CO-7: Classify build execution output

Blocked by: CO-4, CO-5

Blocks: CO-8

- Separate durable build summaries from Progress Displays and Process Passthrough.
- Migrate durable summaries to `UserLog` and build reports to writer-based Command Results.
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
