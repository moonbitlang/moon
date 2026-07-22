---
status: accepted
---

# Separate Command Results from User Logs

MoonBuild will place a small `CommandOutput` interface at the CLI orchestration seam. It will own the command's `UserLog` and provide fallible, locked writes for Command Results on stdout, while renderers below that seam accept a writer and library code prefers returning values. Process Passthrough, Progress Displays, and tracing remain separate because combining their different filtering, ordering, and byte-preservation requirements would create a mode-switched interface and make incremental migration unsafe.

## Consequences

- `--quiet` and `--verbose` affect User Logs but never suppress a Command Result.
- The quiet user-log level is error-only, whether selected explicitly or by a command default. Commands may retain different default user-log levels while they migrate.
- Command Result write failures are returned to the command instead of being discarded.
- One logical result can hold the stdout lock for its complete render, preventing MoonBuild-authored fragments from interleaving.
- The build engines do not receive the whole `CommandOutput` merely to print; they return data or accept the narrower `UserLog` or writer their operation requires.
- Existing direct output migrates command by command. Explicitly classified passthrough, progress, and tracing sites are not mechanical conversion targets.

## Considered Options

- A generic output trait spanning stdout, stderr, progress, child processes, and tracing was rejected because callers would need to understand unrelated modes and ordering rules.
- Generic or boxed writers stored throughout the build graph were rejected because they would spread lifetime, synchronization, and object-safety concerns through planning code that does not own CLI output policy.
- Replacing every print site in one rewrite was rejected because output compatibility could not be reviewed or bisected one command family at a time.
