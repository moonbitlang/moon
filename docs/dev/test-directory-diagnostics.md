# Diagnose test-directory overhead

`moon_test_util::test_dir::TestDir` can record its lifecycle and retain its
workspace for inspection. Both options are off by default. They work with the
existing test runner and do not change snapshots or decide whether a test passes.

| Environment variable | Value and effect |
| --- | --- |
| `MOON_TEST_TRACE_DIR` | Output directory for one JSONL file per workspace. Events are written as phases start and finish, including on ordinary panic unwinding. |
| `MOON_TEST_KEEP_DIR` | Parent directory in which to create and retain workspaces. All workspaces created by that invocation remain, including partially copied fixtures when setup fails. |

An empty value disables either option. Use output directories outside the source
fixtures. Relative paths use the test process's working directory (Cargo normally
uses the crate root); the examples below select absolute output directories.
Keep retention scoped to a filtered test; remove its saved workspaces
when finished. Retention is explicit, so it also works when a custom runner
catches failures before dropping the directory.

## Collect an unchanged-workload baseline

Collect the existing fixture scopes before optimizing their preparation. Keep
commands, assertions, test selection, concurrency, and trace settings fixed for
later comparisons. Disable workspace retention while measuring: it changes the
creation location and skips cleanup. Use fresh trace output directories to avoid
mixing runs, and keep their revision and CI step outcomes with the results.

In PowerShell, from the repository root:

```powershell
$env:MOON_TEST_TRACE_DIR = Join-Path (Get-Location) 'target/test-dir-traces'
Remove-Item Env:MOON_TEST_KEEP_DIR -ErrorAction SilentlyContinue
try {
    cargo test -p moon --test mod test_cases::test_filter::test_moon_test_filter_index_with_auto_update -- --exact
} finally {
    Remove-Item Env:MOON_TEST_TRACE_DIR
}
```

For a Unix shell, set those variables on the filtered command itself:

```sh
MOON_TEST_TRACE_DIR="$PWD/target/test-dir-traces" MOON_TEST_KEEP_DIR= \
  cargo test -p moon --test mod test_cases::test_filter::test_moon_test_filter_index_with_auto_update -- --exact
```

Keep the normal test output alongside the trace. Each JSONL record identifies
the process, Rust test thread, source fixture, and workspace. Its `event` contains:

| Phase | What `elapsed_us` measures on completion |
| --- | --- |
| `create` | Create the temporary directory, including preparing the retention root when requested. |
| `copy` | Copy the complete fixture into the workspace. Only copied fixtures emit this phase. |
| `cleanup` | Delete the workspace and generated artifacts, or mark it `retained` without deleting it. |

Phase states are `started`, `ok`, `error`, and `retained`.
Failures in creation, copying, and cleanup include error text. A phase's `ok`
does not mean the test passed; use the test runner's result. An unmatched
`started` event can help locate a hang or termination, but is not proof of its
cause. Cleanup errors are recorded without introducing a panic. Trace write
failures are best effort and do not change the test result.

Events use separate files to avoid a shared write lock. Trace writes are outside
the measured phases, but recording still adds I/O to the run. Workspaces may
overlap in time, so summing their durations does not give total job wall time.

For a separate inspection run, set `MOON_TEST_KEEP_DIR` to a saved-workspace
directory before the filtered command and unset it afterwards. This retains
passing and failing workspaces, but its `cleanup: retained` records must not be
used as cleanup timing samples.

## Choose what to isolate

When a test writes only build outputs, use `Fixture::readonly(case)` with
the usual `get_stdout` or `moon_cmd` helpers. The fixture reads the checked-in
source with the checkout's normal permissions and gives each test its own
`--target-dir`. All commands on that fixture
reuse the same build, including after `in_subdir("A")` changes the working
directory. In `test_async_test_inline`, `moon.work` supplies the local async
dependency. With `MOON_TEST_KEEP_DIR`, the retained directory contains the
build. To reproduce from `crates/moon/tests/test_cases/moon_test`, set `target`
to the absolute path of its `_build` subdirectory:

```sh
moon -C async_test_inline test --target-dir "$target"
```

Use `Fixture::copied(case)` when a test edits files, promotes snapshots with `-u`,
installs project dependencies into `.mooncakes`, or runs a script or test
that writes into the project. For example, the [JS promotion test](../../crates/moon/tests/test_cases/snapshot_testing/mod.rs)
uses `Fixture::copied("snapshot_testing.in")` so it can check rewritten source and
three snapshot files in its own copy. Also keep the original layout when the
test asserts the default build path or sets its own target directory.
