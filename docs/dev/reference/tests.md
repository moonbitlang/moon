# `moon test` execution flow

This document summarizes what the current `moon test` implementation does. The behavior is
based on `crates/moon/src/run/runtest.rs`, `crates/moon/src/run/runtime.rs`, and
`crates/moon/src/cli/test.rs`. Treat this as internal documentation: user-facing
behavior can change.

## Build + run pipeline

1. The CLI resolves packages and test targets (via Rupes Recta build planning). Each
   selected `BuildTarget` produces two artifacts: the executable (`make_executable`) and
   a JSON metadata file (`generate_test_info`).
2. `run_tests` (in `runtest.rs`) calls `gather_tests`, which pairs those artifacts,
   sorts them by executable path (to match legacy ordering), and then iterates over
   each test binary.
3. Every executable is launched through `run_one_test_executable`. The runner:
   - Parses the metadata (`MooncGenTestInfo`) to know which files and indices exist.
   - Applies the CLI filter to build a `TestArgs` payload that lists the test ranges
     to execute.
   - Builds a backend-specific command (see below) and optionally prints it when
     `--verbose` is enabled.
   - Captures the test output between the `MOON_TEST_DELIMITER_{BEGIN,END}` markers and
     turns each JSONL line into `TestStatistics`. Coverage sections delimited by
     `MOON_COVERAGE_DELIMITER_*` are written to `target/moonbit_coverage_<timestamp>_<rand>.txt`.
4. The collected `TestCaseResult`s are merged per build target inside
   `ReplaceableTestResults`. This structure keeps the latest result per
   `(target, file, index)` pair so later reruns can overwrite earlier data. The
   final report is rendered either as compact text (default) or JSON lines (when
   `--test-failure-json` is on).

## Expect / snapshot promotion loop

> TODO: this behavior is suboptimal. See:
>
> - https://github.com/moonbitlang/core/issues/2684
> - https://github.com/moonbitlang/moon/issues/1310

`moon test --update` enables an iterative promotion workflow for expect tests and
snapshot tests. The CLI enforces a single target backend in this mode (updating
multiple backends at once would diverge binary outputs) and disallows patch
files.

1. After the initial run, `perform_promotion` scans the aggregated
   `ReplaceableTestResults`. For every `ExpectTestFailed` or `SnapshotTestFailed`
   case it:

   - Records the owning `BuildTarget`, file path, and index in a `PackageFilter`.
   - Batches the failure payloads and forwards them to `apply_expect` /
     `apply_snapshot`, which actually rewrite the `.expect`/snapshot files on disk.

2. If the filter is empty, promotion stops immediately. Otherwise the runner:

   - Checks the `--limit` counter (default 256 passes) to avoid infinite loops.
   - Rebuilds just the affected test artifacts by cloning the saved build graph
     and calling `execute_build_partial` with the target nodes returned from the
     filter.
   - Re-runs the filtered subset of tests by wrapping the `PackageFilter` inside a
     temporary `TestFilter`. Only the promoted cases are executed, which keeps
     reruns fast even for large suites.
   - Merges the rerun results back into the main `ReplaceableTestResults` so the
     final summary reflects the updated outcomes.

3. The loop repeats until either the filter is empty or the pass count hits
   `--limit`. When the limit triggers the runner stops promoting and leaves any
   remaining failures in the final output.

During promotion the expect/snapshot helpers in `moonbuild::expect` are
responsible for touching files. The CLI does not stream diffs; failures are still
rendered via `render_expect_fail` / `render_snapshot_fail` to provide context when
`--update` is not active or when a promotion pass still fails.

## Commands per backend (and how to specify test cases)

`command_for` (in `runtime.rs`) hides the differences between target backends. The
runner always records a `CommandGuard` that cleans up temporary drivers when necessary.

### Wasm and WasmGC

- Runner: `moonrun` binary from the active toolchain.
- Invocation: `moonrun --test-args <json> <artifact>.wasm --`.
- The JSON payload is a serialized `TestArgs`, so the runtime knows which files and
  indices to execute. Everything after `--` is forwarded to the MoonBit program.

### JavaScript

- For plain `moon run`, Node reads the emitted `.js`. For tests, the runner synthesizes
  a temporary CommonJS driver next to the compiled `.cjs` file.
- The driver imports the original artifact, sets `testParams` and `packageName`, and
  then invokes the generated test harness.
- Execution command: `node --enable-source-maps <temp_dir>/driver.cjs '<TestArgs JSON>'`.
  The temporary directory also contains a throwaway `package.json` so Node does not try
  to interpret surrounding workspace settings (`type: module`). The directory is deleted
  once the command finishes.

### Native and LLVM

- The compiled executable is run directly.
- Test selection is encoded via `TestArgs::to_cli_args_for_native`, which produces a
  `/`-separated list of `filename:start-end` segments. Each range follows half-open
  semantics (`start` inclusive, `end` exclusive) just like the metadata indices.
- Example argument: `math.mbt:0-5/io.mbt:2-3`.

### Native (tcc-run)

- When the build graph enables `try_tcc_run`, tests can be executed by TCC script mode.
- Command: `<internal tcc> @<artifact>.rsp <native test args>` where the response file
  contains the linker steps needed to run the temporary binary. The same CLI payload as
  native/LLVM is appended so the harness can filter cases.

## Narrowing which tests run

Filtering is handled by `TestFilter` (`runtest/filter.rs`). It stores allowed
`BuildTarget`s, then optional files, then optional index sets.

### Package selection

- `moon test --package foo --package bar` performs fuzzy matching against package fully
  qualified names. Each match becomes a `BuildTarget` for every applicable test kind
  (inline, whitebox, blackbox).
- If multiple packages match, file- or index-level filters are rejected (the CLI
  surfaces an error) because the runner would not know which target to narrow down to.

### File-level filtering

- `--file path/to/test.mbt` narrows the selected packages to a specific file. It can
  only be used together with exactly one `--package`. Alternatively, supplying a
  positional `PATH` (e.g. `moon test src/foo/test.mbt`) activates the same logic—`PATH`
  is resolved, mapped to its owning package, and treated like `--package PKG --file PATH`.
- Files with inline tests may map to multiple build targets (inline tests plus doc tests).
  `TestFilter::add_autodetermine_target` inspects the file type to decide whether to
  request inline, whitebox, or blackbox targets.

### Individual test indices

- `--index N` selects a single inline test (the index is taken from the generated
  metadata, displayed in failure output as `#N`).
- `--index A-B` selects a left-inclusive right-exclusive range.
- `--doc-index N` selects a single doc test block.
- When a positional `PATH` points to a file, the indexes can be provided without an
  explicit `--file`. Otherwise, the CLI requires `--file` whenever `--index` or
  `--doc-index` is used, enforcing the “package → file → index” hierarchy.
- Each selected index becomes a one-element range (`N..N+1`) inside `TestArgs`, which
  is why skipped tests can still be forced to run when explicitly targeted.

### Benchmarks and skips

- `moon bench` reuses the exact same plumbing but passes `bench = true` to the filter,
  so only benchmark entries (`with_bench_args_tests`) are collected.
- `--include-skipped` keeps skipped cases in the computed ranges. Without it, skipped
  tests are excluded unless their indices are explicitly listed.

By understanding the stages above you can reason about how a specific CLI invocation
translates to concrete commands and why certain combinations of flags are rejected.
