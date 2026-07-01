# Behavior of `moon … --dry-run`

> **This output is intentionally unstable and meant only for maintainers.**
> Do not build tools or workflows that depend on the exact formatting or ordering.

- **Deterministic order**. Build commands are printed in a topologically sorted order that corresponds to the build plan selected for the invocation.
- **Unix-style command lines**. Every command line is rewritten using Unix shell quoting, even on Windows hosts.
- **Backslash normalize**. Backslashes `\` in the commandline is normalized to forward slash `/`.
- **Home directory masking**. Any occurrence of the Moon home directory (`~/.moon` or a custom `$MOON_HOME`) is rewritten to the literal `$MOON_HOME`.
- **Project-relative paths**. Paths that live under the project root are emitted as a relative path from the project root, instead of absolute paths.
- **Toolchain binary aliases**. Known Moon toolchain executables (e.g. `~/.moon/bin/moonc`) are shortened to their bare names (`moonc`). Other executables keep their original paths.
- **`moon run --dry-run` extras**. After the build commands, the dry-run output also prints the command that would execute the produced binary (typically `moonrun`, `node`, or the final executable).
- **`moon test --verbose` extras**. With `--verbose` set, `moon test` print the command that is executed for each test case.

## Pasteability and privacy

A reasonable product expectation is that dry-run output should be possible to
copy into the same shell and execute, unless a command explicitly documents a
different preview-only mode. Current output does not fully satisfy that
expectation.

The rendered text is command-shaped, but the current renderer also rewrites
machine-specific paths for stability, snapshot review, and privacy. For example,
`$MOON_HOME` is emitted as a literal marker and known toolchain executables may
be shortened to bare names such as `moonc`. Depending on quoting and the
caller's environment, copying the output directly into a shell may not reproduce
the command that `moon` would execute internally.

Any redesign should keep both goals in view:

- **Pasteability.** If dry-run emits `$MOON_HOME`, `$MOON_TOOLCHAIN_ROOT`, or a
  shortened tool name, the output must either be valid in the user's current
  shell environment or print enough prelude/setup for the following commands to
  work.
- **Privacy.** Dry-run output is commonly copied into issues, review comments,
  and snapshots. It should avoid exposing user-specific absolute paths such as
  home directories, workspace roots, custom toolchain paths, or local package
  cache paths unless the user opts into raw paths.

The likely direction is a privacy-preserving pasteable form: keep project paths
relative where possible, use stable symbolic variables for home/toolchain paths,
and ensure those variables are shell-expandable rather than inert snapshot
markers. The current implementation predates that contract and should be treated
as a compatibility baseline, not as the final design.

## Where the command text comes from

For RR builds, `moonbuild-rupes-recta` assembles most compiler/tool invocations
as structured argument vectors during lowering, then flattens them into the n2
graph command string. The dry-run printer reads the selected n2 build nodes and
prints their stored command lines after applying the dry-run path normalizer.

There are exceptions:

- user/configured prebuild commands are verbatim shell command strings;
- `moon run --dry-run` also prints the final run command assembled outside the
  build graph;
- `moon cram --dry-run` prints the cram delegation command assembled in the cram
  CLI path.

## Why the path normalizer exists

The path normalizer came from test/debug output, not from a fully specified
user-facing pasteable-script design.

- `9911da66b feat(rr): Allow tests to dump build graph to file` added the
  `MOON_TEST_DUMP_BUILD_GRAPH` hook so integration tests could snapshot the n2
  graph selected by dry-run invocations.
- `e2445e409 feat(debug): Normalize dumped build graph` introduced path
  normalization for that dumped graph. The immediate goal was stable snapshots:
  remove local project roots, hide machine-specific Moon home paths, and make
  tool paths comparable across machines.
- `8c0fc5a58 refactor: unify to unix quoting when printing dry-run and graph cmp`
  aligned dry-run output and graph comparison around Unix-style quoting.
- `8b3525c00 fix: normalizer semantics` refined the replacement order and
  semantics, including project-relative paths, `$MOON_HOME`, and tool aliases.
- `98d282bfa refactor: de-productionize moonbuild-debug` moved the remaining
  runtime pieces into `moonbuild::dry_run` because the compiled `moon` binary
  still needs the integration-test graph-dump hook. That move was not intended
  to make graph dumping or broad tool alias resolution part of normal build
  semantics.

## Known debt

- The normalizer currently builds its alias table from all known Moon tool
  binaries. This can force unrelated lazy binary paths to resolve just because a
  dry-run command is being printed.
- The `MOON_TEST_DUMP_BUILD_GRAPH` traversal still walks raw n2 `ins.ids`, which
  collapses explicit, implicit, order-only, and validation/lazy inputs. That is
  graph-dump test infrastructure and should not define dry-run dependency
  semantics.
