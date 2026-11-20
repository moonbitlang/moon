# Writing and maintaining tests

Start by naming the behavior that should fail if the implementation is wrong.
Choose its execution boundary below, then the assertion that makes that behavior
visible. A snapshot is an assertion style, not a separate test level.
For how `moon test` itself works, read the [execution reference](reference/tests.md).
Cleanup should make protected behaviors easier to find and understand while
preserving coverage. Fewer tests or snapshots is not a success criterion.

## Choose the test level

For new coverage, choose the smallest boundary that can prove the contract.
Ordinary tests cover rule variations; E2E tests cover workflows whose correctness
depends on the real components working together. Organizing existing tests does
not by itself justify changing which boundaries they exercise.

| Level | What it should prove | Execution and examples |
| --- | --- | --- |
| Ordinary: unit or phase/component | A rule or phase produces the correct values, errors, selection, or Build Plan | Call the owning API, with fixtures where needed; no compiler/runtime execution. [command selection](../../crates/moon/src/cli/invocation.rs), [resolver](../../crates/mooncake/src/resolver), [planner](../../crates/moon/src/cli/tests/planner/mod.rs). |
| CLI integration | The executable connects argv, dispatch, exit status, streams, or process lifecycle correctly | Run the real CLI with a small fixture; controlled child tools can isolate forwarding. [Validation smoke test](../../crates/moon/tests/test_cases/debug_flag_test/mod.rs), [testcli forwarding](../../crates/moon/tests/test_cases/testcli.rs). |
| End-to-end (E2E) | A user workflow succeeds with the real downstream components and state it depends on | Exercise the complete workflow, including real compilation/execution where required. [inline async test](../../crates/moon/tests/test_cases/moon_test/mod.rs), [multi-target execution](../../crates/moon/tests/test_cases/targets/mod.rs). |

Reuse discovered packages for planner comparisons when discovery is not the
contract. Select the backend before resolving backend-specific imports, as the
command does. For dependency-free single-module planning scenarios,
use `PlanningFixture::with_core_dependency` to keep the standard-library input fixed.
Its [core declarations](../../crates/moon/src/cli/tests/planner/fixtures/core_dependency/README.md)
exercise dependency edges without reading the installed SDK's core manifests.
This deliberately stops tracking the SDK's exact package composition; it is
appropriate when that composition is not the contract under test. Keep real SDK
compatibility checks in tests that execute the compiler/runtime.
The [planning fixture](../../crates/moon/src/cli/tests/planner/fixture.rs)
is not a substitute for testing CLI wiring, execution, or file mutation.
These tests live within `cli` so command planning entry points need not be
exposed to the rest of the binary. Their execution boundary is unchanged.

These are testing responsibilities, not Cargo target names.
`tests/test_cases` still contains both CLI integration and end-to-end tests.
Calling a command with `--dry-run` still crosses CLI, discovery, and planning
boundaries; it is not a unit test. Likewise, a Rust test need not be pure just
because it lives beside the implementation. Avoid network or external-tool
requirements unless those are the behavior being tested; state them by the case.
Filesystem access alone does not make a test E2E: a CLI test that removes seeded
cache files checks a filesystem boundary; building a project, cleaning its real
artifacts, and successfully rebuilding checks the complete workflow.
Use `Fixture::readonly(case)` for tests that share source and need a private build
target. Use `Fixture::copied(case)` when a test writes into its project or
checks the default build path. For examples,
timing, and retaining artifacts for inspection, see
[Test-directory diagnostics](test-directory-diagnostics.md#choose-what-to-isolate).

### Core fixture roles

Core has two distinct build roles. A local `moonbitlang/core` project must not
receive another core dependency; its packages produce local artifacts. An
ordinary project implicitly imports builtin/prelude and consumes installed core
artifacts. See [special cases](reference/rr-special-cases.md) for the production
rules and [test dependencies](reference/tests.md) for generated-driver behavior.
Choose a fixture for its role and execution boundary:

| Fixture | Contract and boundary |
| --- | --- |
| [core_dependency](../../crates/moon/src/cli/tests/planner/fixtures/core_dependency/README.md) | Core-specific implicit imports, installed artifact paths, and no local core compilation. Also supplies a fixed environment for ordinary-project package-selection and profile tests. |
| [core_project](../../crates/moon/tests/test_cases/core_project/README.md) | Core as the root module: module synchronization, coverage source merging and import exceptions, local artifact production, and source/blackbox compiler commands. No compiler execution. |
| [dummy_core](../../crates/moon/tests/test_cases/dummy_core) | Existing core-project backend matrix: check/build/test planner snapshots, CLI bundle/coverage dry-runs, and package-index paths. Keep these assertions when adding smaller core cases. |
| [query_symbol.in/core](../../crates/moon/tests/test_cases/query_symbol.in/core) | A compilable core project for the real IDE documentation query. Primitive declarations and `builtin.dummy` keep the queried project small; this is not a generic planner fixture. |

Specialized core scenarios, such as blackbox driver rewriting and proof support,
keep their own fixtures. A declaration-only core cannot replace a scenario that
needs real compiler, runtime, or IDE behavior.

Core scenarios should test rules specific to core. Test shared behavior such as
package filtering, profile selection, or dependency-closure deduplication with
ordinary projects or at the owning lower layer. Using core as the environment
does not make its package composition part of that test's contract. Add a backend
matrix only when the scenario distinguishes backend-dependent behavior; otherwise
select one backend explicitly.

## Make the assertion explain the contract

Use a descriptive test name and a short explanation of the scenario or invariant
beside the assertion. Explain why the fixture or expected difference matters,
rather than narrating the Rust code. A reviewer should not have to infer the
purpose from a long output file.

- For a few facts, compare typed values, complete collections, or parsed output.
  Check both inclusion and exclusion when testing selection. Avoid a snapshot of
  every default field in a configuration object to test one flag.
- For output that users read, keep a short exact inline snapshot next to the
  command and its explanation. Use `expect_test::expect!` for rendered values
  and direct snapbox assertions for processes; the
  [snapbox skill](../../.agents/skills/snapbox-testing/SKILL.md) has API examples.
- For a complete graph or long stable output that is itself the contract, use
  `expect_file!`. The [graph assertion helper](../../crates/moon/tests/support/build_graph.rs)
  captures and normalizes process-level graph dumps from a configured `moon_cmd`.
  Planner tests can capture the graph through their existing fixture.
- If only a projection of a graph matters, assert that projection explicitly and
  explain what it excludes. Do not replace a full snapshot with loose substring
  checks, or hide meaningful lines behind broad wildcards.

Keep exit status and stdout/stderr distinct in process tests. Redact only
incidental volatility such as a temporary root; preserve diagnostic text, path
suffixes, ordering, and other behavior the test promises to check.

## Write E2E tests as explained scenarios

Use Given/When/Then to describe the starting state, user action, and observable
outcome beside the corresponding setup, command, and assertions. Name one user
goal per case. Keep dependent steps together when their state transitions are
the contract; split unrelated goals. State the real tools/backend required and
isolate the mutable state so another test cannot supply the expected result.

For compiler/runtime output, keep pure decoding and formatting rules in unit
tests. Exercise locating, extracting, and replacing text in real generated
output through E2E tests. For example, stack-trace demangling needs a compiled
program that fails in the real runtime: a hand-written stack string only proves
the transformation for that assumed format. Prefer coverage of the user-visible
stack trace over a standalone demangle command when stack rendering is the
behavior being changed.

Make the feature understandable without reading its Rust steps:

- Put the broad behavior and shared starting context in `Background`. Explain
  an output format there when readers need it to interpret several scenarios.
  Put case-specific details and edge cases in the relevant `Scenario`.
- Identify the source fixture and its location, plus the source contents or
  metadata needed to understand the result. A `Given` that prepares source
  files should describe those sources. Establish a compiled executable through
  an actual build before describing it as available to run.
- Match step wording to the actions performed. Release optimization, target
  selection, and symbol retention describe compilation. Execution consumes the
  resulting artifact. If a command both compiles and runs, say so; if the
  scenario uses separate commands, show and check each stage.
- Review each piece of prose for the context it adds beyond the scenario name,
  steps, and assertions. Keep explanations of expected behavior and non-obvious
  test data. Remove redundant command inventories and restatements of visible
  expectations. Moving text between feature and scenario is useful only when
  the information belongs at that scope.
- When the origin of displayed information matters, make competing inputs
  distinguishable. For example, give a source map a different name and source
  position from the Wasm artifact, then assert the resulting frame. This proves
  which input was consumed and which supplied each displayed value.

Apply the same discipline to phase tests. When a rule crosses phases, give each
boundary its own focused assertion: an implicit import must appear in the
resolved dependency graph, its interface must be tracked as a compile input,
and the generated compiler command must pass the intended `-i` alias. The
[core dependency scenarios](../../crates/moon/src/cli/tests/planner/core_dependency.rs)
keep builtin and prelude separate at each boundary. A correct command alone
does not prove that changing its input will invalidate the build.

For example, the [inline async fixture](../../crates/moon/tests/test_cases/moon_test/async_test_inline/inline_test_file.mbt)
has one empty `async test {}` in an ordinary source file. Running
`moon -C async_test_inline test` must exit successfully and report one passing
test. Checking the count catches a regression where the driver silently skipped
the test. The [Rust test](../../crates/moon/tests/test_cases/moon_test/mod.rs)
uses a local async implementation supplied by the fixture's workspace.

BDD describes the behavior; a Scrut-style transcript puts explanation around
executable commands and expected output. Neither determines the test level.

For Cucumber E2E tests, start with the
[topic index](../../crates/moon-e2e/README.md). It links each feature to its Rust
steps and explains how to run a topic or scenario. The independent `moon-e2e`
crate runs the real CLI, compiler, and runtime. Unit and phase tests remain in
the owning crate; using Gherkin does not change their execution boundary.

Existing source fixtures stay in `moon/tests/test_cases`. Package-selection
scenarios share read-only source and own a private build directory. Scenarios
that generate interfaces or package archives use private source copies through
`moon_test_util::test_dir::TestDir`. Moon's ordinary tests keep their existing
`Fixture` and command helpers.

The two exclusion fixtures retain their `moon.test` files as archive input:
those filenames appear in the expected package lists. These files are no longer
executed as transcripts. The custom transcript runner has been removed; all its
active scenarios now run in `moon-e2e`. Adjacent Rust tests for quiet output,
removed manifest options and ignore-file precedence remain separate.

Every protected expectation must be visible in the feature: exit status,
stdout/stderr (including empty streams), diagnostics, order, compiler arguments,
and file results. Steps execute actions and compare the supplied expectations;
they must not hide additional behavior assertions in Rust snapshots or output
templates. Prefer explicit source-selection, dependency and generated-interface
assertions over complete compiler-command or package-index snapshots. Avoid
directory-tree assertions unless directory contents are the behavior being
tested, and do not snapshot compiled `.core` contents. See the index's
[reading conventions](../../crates/moon-e2e/README.md#read-the-expectations).
Update feature expectations by reviewing the behavior and editing the feature;
`UPDATE_EXPECT` does not rewrite Gherkin steps or tables.

The E2E crate is a default workspace member, so normal CI `cargo test` includes
it. It has its own Cucumber runner; use its documented topic/name filters and
keep libtest flags on ordinary Rust test targets.

The E2E command helper asks Cargo for the `moon` and `moonrun` executables once
per process, using the workspace manifest and the default target layout to
reuse normal build artifacts. It uses Cargo's reported paths rather than
looking for an installed `moon` or guessing `target/debug`. Cucumber is only a
dev dependency of `moon-e2e`; testing `moon` alone does not compile it.

Keep independent examples in separate scenarios, and keep dependent workflow
steps in one scenario. Compare migration timings: splitting a transcript's
shared build directory into isolated scenarios can repeat compilation.

Use ordinary Rust tests for focused phase behavior. The existing
[native Moon Cram fixture](../../crates/moon/tests/test_cases/cram/tests/cram/main.mooncram)
also shows an executable shell transcript. Native `moon-cram` and the
`moon testcli` WASM runner have different command models. Before migrating any
format, verify it uses the Cargo-built CLI, isolates fixtures, checks the
intended streams/status, and runs on supported platforms.

## Clean up an existing test

1. Identify each protected behavior, input variation, and execution boundary.
   Record the owning tests and assertions, including backend, status, streams,
   file changes, and relevant defaults.
2. Prefer renaming, grouping, explaining, and splitting independent scenarios
   while preserving their assertions. Keep a sequence together when its state
   transitions are the contract, such as promotion or incremental builds.
3. Treat moving a case to a narrower boundary as a separate coverage decision.
   Map its assertions to the replacement and keep the process or E2E checks that
   remain necessary. A direct parser call does not prove executable wiring even
   when it checks the same diagnostic.
4. Remove an assertion only after establishing that its contract is duplicated
   by retained coverage, or explaining why the constraint is obsolete. A large
   snapshot may need clearer organization rather than fewer expectations.
   Error kinds alone do not cover diagnostic text or command dispatch; explicit
   inputs do not cover CLI defaults.
5. Run the affected cases and review the before/after assertion mapping. Judge
   the cleanup by clearer behavior ownership and failure diagnosis, not counts.
   Inspect failures before updating expected output.

Do not classify an entire suite as obsolete from its name or file size. Keep
coverage ownership and remaining cleanup candidates beside the tests or in the
active change that evaluates them.

## Run and review

Initialize pinned submodules as described in [Contributing](contributing.md).
Run the smallest relevant test target and filter, for example:

```sh
cargo test -p moon --bin moon handoff_parsing_is_pure_and_keeps_the_program_argument_tail
cargo test -p moon --test mod cli_reports_selector_conflicts_before_planning
cargo test -p moon --bin moon cli::tests::planner
cargo test -p moon --test mod test_cases::moon_test::test_async_test_inline -- --exact
cargo fmt --all --check
cargo clippy -p moon --all-targets -- -D warnings
```

For an intentional expectation change, set `UPDATE_EXPECT=1` for expect-test or
`SNAPSHOTS=overwrite` for snapbox on the **filtered** test command only. In
PowerShell, set the corresponding `$env:` variable for that command and remove
it afterwards. Never promote snapshots with missing or outdated submodules.
Inspect the diff, then rerun with the update variable unset. An accepted snapshot
is evidence only after its expected behavior has been reviewed.

Use the [repository checks](contributing.md#validate-a-change) before submission;
broaden verification when a change crosses more than the tested boundary.
