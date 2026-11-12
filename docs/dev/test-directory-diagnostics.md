# Diagnose test-directory overhead

`moon_test_util::Fixture` can record its lifecycle and retain its
workspace for inspection. Both options are off by default. They work with the
existing test runner and do not change snapshots or decide whether a test passes.

| Environment variable | Value and effect |
| --- | --- |
| `MOON_TEST_TRACE_DIR` | Output directory for one JSONL file per workspace. Events are written as phases start and finish, including on ordinary panic unwinding. |
| `MOON_TEST_KEEP_DIR` | Parent directory in which to create and retain workspaces. All workspaces created by that invocation remain, including partially copied or written fixtures when preparation fails. |

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
| `setup` | Create parent directories and write the inline files supplied to one `Fixture::with_files` call. |
| `cleanup` | Delete the workspace and generated artifacts, or mark it `retained` without deleting it. |

Phase states are `started`, `ok`, `error`, and `retained`.
Failures in creation, copying, inline file setup, and cleanup include error text.
A phase's `ok` does not mean the test passed; use the test runner's result.
An unmatched `started` event can help locate a hang or termination, but is not proof of its
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

For small or dynamically generated projects, keep the input beside its test:

```rust
use moon_test_util::Fixture;

let project = Fixture::new().with_files([
    ("moon.mod", "name = \"test/example\"\n"),
    ("src/moon.pkg", ""),
    ("src/lib.mbt", "pub fn answer() -> Int { 42 }\n"),
]);
```

`with_files` writes paths relative to the current working directory, creates
missing parent directories, and records a separate `setup` phase. It uses the same
retention and cleanup policy as copied fixtures, including after a write fails.
It rejects writes into readonly fixtures. The source tree remains writable by
ordinary filesystem APIs; readonly is a test isolation contract, not an OS
permission setting.
The shared helper is available to independent integration test targets as well
as the main test suite; see the [LLVM tests](../../crates/moon/tests/llvm_backend.rs).
Keep later edits explicit when they are steps of the tested workflow, as in the
[stdlib lifecycle test](../../crates/moon/tests/stdlib_abort.rs).

Only operations performed by `with_files` are included in its `setup` timing.
Direct `std::fs` calls outside it are not measured; plain `tempfile` directories
do not participate in these diagnostics or retention. Passing an already
formatted string also excludes its construction time. Do not interpret summed
`copy` and `setup` durations as all fixture preparation in the test suite.

For existing source trees, `Fixture::copied(source)` and
`Fixture::readonly(source)` accept paths supplied by the caller. In the main
Moon test suite, use `test_case_path("case")` to resolve a checked-in case.

Construct commands with `fixture.moon_cmd(moon_binary)` (snapbox) or
`fixture.moon_process_cmd(moon_binary)` (standard process command), then set any
required environment and arguments. These entry points set both the working
directory and the private target override. A fixture deliberately does not
implement `AsRef<Path>`: treating it as just a working directory would lose the
readonly target override. Use `cwd()` or `join()` for explicit source paths and
`target_dir()` to inspect the optional command override. Keep the fixture alive
until its commands finish.

When a test writes only build outputs, use `Fixture::readonly(source)` with
these command methods or the main suite's `get_stdout` and `moon_cmd` helpers.
The fixture reads the checked-in source with the checkout's normal permissions and gives each test its own
`--target-dir`. All commands on that fixture
reuse the same build, including after `in_subdir("A")` changes the working
directory. In `test_async_test_inline`, `moon.work` supplies the local async
dependency. With `MOON_TEST_KEEP_DIR`, the retained directory contains the
build. To reproduce from `crates/moon/tests/test_cases/moon_test`, set `target`
to the absolute path of its `_build` subdirectory:

```sh
moon -C async_test_inline test --target-dir "$target"
```

Use `Fixture::copied(source)` when a test edits files, promotes snapshots with `-u`,
installs project dependencies into `.mooncakes`, or runs a script or test
that writes into the project. For example, the [JS promotion test](../../crates/moon/tests/test_cases/snapshot_testing/mod.rs)
uses `Fixture::copied(test_case_path("snapshot_testing.in"))` so it can check
rewritten source and three snapshot files in its own copy. Also keep the original layout when the
test asserts the default build path or sets its own target directory.

The legacy `TestDir` and path-based command adapters remain for existing tests.
New or migrated scenarios should use `Fixture`; the compatibility adapters can
be removed when their callers have migrated. Low-level directory tests may use
`tempfile` for independent source, trace, and retention roots so the mechanism
under test does not control its own test sandbox.
