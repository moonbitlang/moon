# Moon E2E scenarios

Start with the behavior you want to understand. Each feature describes its CLI,
starting project, actions and expected results. Read its Rust steps only to
inspect how the scenario is executed; they do not own additional expected output.
These scenarios exercise the real Moon CLI and downstream tools. Unit and phase
tests remain in the crate that owns the code.

## Find a scenario

| Topic / question | Feature | Rust steps | Filter |
| --- | --- | --- | --- |
| **Directory permissions:** Do recursive grants preserve child restrictions and reject unsupported mutations before effects? | [Filesystem permissions](features/testing/filesystem_permissions.feature) | [filesystem_permissions.rs](tests/behavior/filesystem_permissions.rs) | `@filesystem_permissions` |
| **Building libraries:** Does the preview compile dependencies first and pass their interfaces? Can the compiler build both libraries? | [Library build](features/build/library_build.feature) | [library_build.rs](tests/behavior/library_build.rs) | `@library_build` |
| **Cleaning builds:** Can a module or workspace remove its build outputs and build again? | [Clean and rebuild](features/build/clean.feature) | [clean.rs](tests/behavior/clean.rs) | `@clean` |
| **Selecting sources:** Do backend switches and default/explicit profiles select the right source inputs? | [Conditional compilation](features/build/conditional_compilation.feature) | [conditional_compilation.rs](tests/behavior/conditional_compilation.rs), [preview.rs](tests/behavior/preview.rs) | `@conditional_compilation` |
| **Source roots:** Does `source = "."` resolve compiler inputs from the module root? | [Source root](features/build/source_root.feature) | [conditional_compilation.rs](tests/behavior/conditional_compilation.rs), [preview.rs](tests/behavior/preview.rs) | `@source_root` |
| **Generating interfaces:** Where does `moon info` write interfaces for different source layouts, and what do they contain? | [Package interfaces](features/interfaces/package_interfaces.feature) | [package_interfaces.rs](tests/behavior/package_interfaces.rs), [files.rs](tests/behavior/files.rs) | `@package_interfaces` |
| **IDE source information:** Does the index retain inactive files and conditions while its check commands use only active inputs? | [Conditional index](features/interfaces/conditional_index.feature) | [conditional_compilation.rs](tests/behavior/conditional_compilation.rs) | `@conditional_index` |
| **Publishing:** Which files do ignore rules include in the archive, and in what order? | [Packaging](features/publishing/packaging.feature) | [packaging.rs](tests/behavior/packaging.rs) | `@packaging` |
| **Selecting tests:** Which packages run for exact/fuzzy selectors? What happens when none match or a file filter is ambiguous? | [Package selection](features/testing/package_selection.feature) | [package_selection.rs](tests/behavior/package_selection.rs) | `@package_selection` |
| **Updating inline expectations:** Does each backend update only its own source, preserve earlier updates, and pass when rerun without updating? | [Snapshot updates](features/testing/snapshot_updates.feature) | [snapshot_updates.rs](tests/behavior/snapshot_updates.rs), [files.rs](tests/behavior/files.rs) | `@snapshot_updates` |
| **Conditional tests:** Do checks, test drivers and test compilation agree on source, blackbox and doctest inputs? | [Conditional tests](features/testing/conditional_tests.feature) | [conditional_compilation.rs](tests/behavior/conditional_compilation.rs), [preview.rs](tests/behavior/preview.rs) | `@conditional_tests` |
| **Wasm stack traces:** Do executable/test stacks demangle module-qualified functions, honor --no-strip, and retain locations when source maps are unavailable? | [Wasm stack trace](features/testing/wasm_stack_trace.feature) | [wasm_stack_trace.rs](tests/behavior/wasm_stack_trace.rs) | `@wasm_stack_trace` |

Keep features under their topic directory. Add an index entry when introducing
a feature; add a new topic only when the existing topics do not fit. This index
links to the specifications rather than repeating their expected values.

## Read the expectations

- `Given` selects or prepares the named project; `When` performs the described
  action. Conventional actions such as `I build the project` usually mean the
  default command and need no repeated CLI introduction. Describe special
  arguments where they help explain the scenario, in the step or introduction.
  This is a reading convention; step definitions remain the source for how
  commands are executed. Source fixtures stay in `moon/tests/test_cases`.
  Follow the
  [scenario principles](../../docs/dev/testing.md#write-e2e-tests-as-explained-scenarios)
  for fixture context, compilation/execution steps, and explanatory prose.
- Each command has explicit exit-status and relevant output expectations. Check
  empty streams when silence is part of the behavior. Messages, counts, ordering,
  selected inputs and file results belong in the feature, not in Rust constants
  or templates.
- For an error code whose native representation differs, the exit-status step
  states both Unix and Windows expectations. In particular, Moon's `-1` error
  exit is observed as `255` on Unix and `-1` on Windows.
- `stdout/stderr is exactly` compares the entire stream with only line endings
  normalized. Output docstrings include a final newline; additional blank lines
  inside them are significant.
- `stdout/stderr matches` also normalizes path separators and the named roots
  `[WORK_DIR]` (private project) and `[TOOLCHAIN]` (selected SDK).
- Preview steps append `--dry-run --sort-input` to the stated Moon arguments.
  Arguments in the feature are plain whitespace-separated words, not shell
  scripts. Moon renders preview commands with Unix quoting on all platforms;
  the step parser follows that output format.
  Source tables compare **all commands of the named kind**, including duplicates,
  and their complete source input sets; table/source order is not significant.
  Filenames are relative to the row's directory. Empty cells mean no inputs.
  `driver kind` identifies a generator action; `generated drivers` identifies
  each consumed driver as `package:kind`, so consuming another package's driver
  is also a failure.
  Dependency order and interface imports are checked separately where relevant.
- Index tables check file conditions and selected command inputs, without fixing
  JSON formatting, unrelated defaults, SDK link lists or build-directory layouts.
  Source paths for `source = "."` are checked because they are compiler inputs.
- File-content docstrings retain intentional trailing newlines: a blank line
  before the closing delimiter represents a final newline in the file.
  Source files named as unchanged are compared byte-for-byte with their contents
  immediately before that update, including any earlier updates in the scenario.
  Interface creation checks the named file and its contents, without enumerating
  its whole directory.
  Archive file lists are retained because selecting archive contents is the
  behavior under test. No feature snapshots compiled `.core` contents.

Every feature starts with paths to its step definitions. Shared exit-status and
stream assertions live in [command.rs](tests/behavior/command.rs), and exact
file-content assertions in [files.rs](tests/behavior/files.rs). The
[runner](tests/behavior/main.rs) owns per-scenario state and source/build-directory
isolation; [support.rs](tests/behavior/support.rs) locates the real executables and
configures path redactions. No extra behavior expectations belong in these files.

These scenarios replace all active custom `moon.test` transcripts; the old
parser, executor and `UPDATE_EXPECT` transcript writer have been removed.
Review behavioral changes and edit the relevant feature expectation explicitly;
there is no automatic feature rewrite. The two `moon.test` files in exclusion
fixtures remain inert archive inputs, not a second test framework.

Only Cucumber's step/world macros are enabled; optional report formats and tracing
integration stay disabled. The runner reuses Cucumber's existing `futures-executor`
dependency. Its synchronous process and filesystem steps need no Tokio runtime.

## Run a topic or scenario

Run these from the workspace root:

```sh
cargo test -p moon-e2e --test behavior
cargo test -p moon-e2e --test behavior -- --tags @packaging
cargo test -p moon-e2e --test behavior -- --name "Exact full name"
```

The runner discovers features recursively and fails on undefined steps. Cucumber
owns these arguments; libtest flags such as `--exact` and `--nocapture` do not
apply. Ordinary Rust tests keep their existing runners.

For test-level selection, fixture choices and migration rules, see the
[testing guide](../../docs/dev/testing.md#write-e2e-tests-as-explained-scenarios).
