# Test suite strategy

This document describes how tests in `crates/moon/tests/test_cases` should be
split by purpose. The goal is to keep the default suite fast without weakening
coverage.

This is an audit of the current test layout, not a statement that the repo is
already perfectly organized this way.

Nothing in this document implies that the suggested moves have already landed.

## Test kinds

### Unit tests

Use unit tests when the behavior is a pure transformation over Rust values.

Typical examples:

- filter construction and matching
- diagnostic formatting
- shell completion generation
- help text generation
- package/path/backend matching logic

Preferred location:

- next to the implementation module under `crates/moon/src/**`

Signals that a test should be unit-level:

- it does not need a temporary MoonBit project
- it does not need to spawn `moon`
- it only checks data selection, filtering, formatting, or serialization

### Snapshot tests

Use snapshot tests for deterministic text or graph output where the full output
is the behavior.

Typical examples:

- dry-run command graphs
- generated shell completion scripts
- structured CLI output where exact ordering matters

Preferred location:

- unit tests when the output comes from a pure Rust function
- otherwise a narrow integration test that runs one command and snapshots one
  stable artifact

Signals that a test should be snapshot-based:

- the output is long and exact
- line ordering matters
- reviewers need to understand the output as a whole
- redaction can hide incidental volatility without hiding the structure
- replacing it with a handful of `contains` checks would hide regressions

### Assertion-only integration tests

Use assertion-only integration tests when the code must cross a process or
filesystem boundary, but the behavior under test is small.

Typical examples:

- `moon` exits with a helpful error
- one file is created or not created
- a warning goes to stdout vs stderr
- a command contains a specific flag

Preferred location:

- `crates/moon/tests/test_cases/**`

Signals that a test should be assertion-only:

- only a few facts matter
- the assertions are semantic, not just scattered substring checks
- a full snapshot would mostly add noise rather than clarity
- the test exists to validate CLI wiring, not internal algorithms

### End-to-end tests

Use end-to-end tests when the behavior depends on the real MoonBit toolchain,
runtime, backend integration, file rewrites, or multi-step command flow.

Typical examples:

- `moon test --update`
- snapshot/expect promotion loops
- markdown tests
- async test execution
- backend runtime behavior

Preferred location:

- `crates/moon/tests/test_cases/**`

Signals that a test should stay e2e:

- it must compile or execute MoonBit code
- it must verify rewritten source or snapshot files
- it depends on backend/runtime behavior that is hard to fake cleanly

### Slow or external tests

Use a separate bucket for tests that depend on the network, the global machine
configuration, or environment-specific tools.

Typical examples:

- registry update or clone
- git install from GitHub
- toolchain inventory from external binaries

Preferred location:

- still under integration tests, but isolated, explicitly marked, or moved into
  a slower CI job

## General rules

- Prefer the narrowest execution level that still tests the real behavior:
  unit tests over integration tests, and integration tests over e2e tests.
- Choose snapshot tests when a large deterministic output is itself the
  behavior and a full view is more readable than many small checks.
- Choose assertion-only tests when the assertions are semantic and only a few
  properties matter.
- Do not use e2e tests to validate pure selection or formatting logic.
- Do not use long snapshots when only a few semantic invariants matter.
- Do not replace a readable output snapshot with weak `contains` checks that
  make the intent harder to review.
- Split suites by behavior, not by command name.
- Keep one or two e2e smoke tests per feature after moving the stable logic
  downward.

## Test by workflow phase

For command behavior, the most useful split is often not "unit vs integration"
but "which phase of the workflow are we starting from".

When the workflow is modular, tests can start from the narrowest phase that
still covers the behavior under review.

### Phase 0: CLI parsing and normalization

Inputs:

- raw argv

Outputs:

- clap parsing success/failure
- normalized command flags / help text / shell completion text

Use this phase for:

- help text
- shell completion
- clap validation and conflict rules

Do not use this phase for:

- package selection logic
- backend selection logic
- build planning logic

Preferred tests:

- unit snapshots for generated text
- a very small number of CLI process smoke tests for wiring

### Phase 1: Intent computation

Inputs:

- already parsed command options
- explicit path / package / file / index filters
- selected backend
- sometimes already resolved package metadata

Outputs:

- `UserIntent`
- test filters
- patch / input directives
- targeted user-facing errors for invalid selections

Use this phase for:

- package/backend filtering
- path/package/file/index selection
- "unsupported backend" and similar intent-level failures

This is usually the best phase for command-selection tests when the intent is
already clear in the test.

### Phase 2: Project resolution

Inputs:

- source directory
- registry / sync configuration
- std / no-std / coverage flags

Outputs:

- resolved workspace / module / package graph

Use this phase for:

- dependency resolver behavior
- workspace/package discovery
- registry and resolution edge cases

Tests here should prefer injected registries or other explicit test doubles
over full CLI update/install flows when possible.

### Phase 3: Planning and lowering

Inputs:

- resolved project
- explicit user intent
- selected backend
- compile configuration

Outputs:

- `BuildMeta`
- build graph / plan nodes
- planner warnings and compatibility errors

Use this phase for:

- dry-run graph coverage
- backend compatibility checks
- target-specific planner behavior

If the resolved project is not what is under test, resolve once and reuse it
for several planner assertions.

### Phase 4: Execution and mutation

Inputs:

- planned build graph
- artifacts / runtime / filesystem state

Outputs:

- compiled artifacts
- runtime output
- rewritten files
- promoted snapshots / expects

Use this phase only when the behavior truly depends on:

- the MoonBit toolchain
- runtime execution
- file rewriting
- multi-step command flow

## Where a test should start

When writing a new test, pick the latest possible starting phase that still
covers the behavior:

- If the behavior is only text generation, start at phase 0.
- If the test already knows the user intent, start at phase 1 instead of
  re-parsing argv and re-resolving the whole project.
- If the test needs real package/module metadata but not CLI behavior, resolve
  the project once and start from phase 1 or phase 3.
- If the test compares planner behavior across several backends, reuse one
  resolved project and vary only backend + intent.
- If the test needs runtime effects or rewritten files, start at phase 4 and
  keep the coverage focused.

The existing `rr_build::plan_build_from_resolved` entry point is a good model:
it lets tests skip repeated resolve work and focus on intent + planning.

Commands should ideally expose similarly testable seams for:

- "already parsed, compute intent"
- "already resolved, plan"
- "already planned, execute"

## Design guidelines for testable command workflows

Prefer command implementations that look like explicit phase composition:

1. parse / normalize CLI input
2. compute intent
3. resolve project
4. plan from resolved project + intent
5. execute or print

The convenience wrapper may still do all of that in one public function, but it
should be built from smaller entry points that tests can call directly.

Preferred API shape:

- keep typed request values separate from clap structs when practical
- keep `calc_user_intent`-style logic separate from execution
- offer `*_from_resolved` or `*_from_metadata` helpers next to convenience
  wrappers
- let tests inject the registry / resolved project / metadata explicitly

Avoid mode-switched helpers that both infer intent and perform unrelated IO.

## Anti-patterns today

These are the main patterns that currently make tests slower, broader, or
harder to understand than they need to be.

- Subprocess tests used as a proxy for selection or formatting logic.
  The test ends up covering clap parsing, resolution, planning, and output
  rendering just to validate one selection rule.
- Command implementations that combine intent derivation, resolve, planning,
  and execution in one path with no reusable lower-level seam.
- Repeating the same expensive workflow for each backend when the interesting
  variable is only the backend.
- Dry-run stdout substring checks used to validate planner selection behavior.
  This is usually weaker than asserting on explicit intent or snapshotting the
  graph at the planning layer.
- Suites grouped by command name even when the underlying behavior belongs to
  different phases.
- `calc_user_intent` logic that is only reachable through a full command
  workflow, making it awkward to test backend/package/filter behavior directly.

## Concrete improvement directions

When refactoring command code for testability, the most promising directions
are:

- Introduce shared typed "intent request" structs so intent computation is not
  tied directly to clap structs.
- Add more entry points analogous to `plan_build_from_resolved` for commands
  that currently only expose the full workflow.
- Add small fixture helpers for "resolve this test case once" so unit tests can
  exercise several backends or intents against the same resolved workspace.
- Let planner tests start from explicit intent + resolved project rather than
  re-running CLI parsing and package discovery.
- Keep only a small number of CLI smoke tests per feature once lower phases are
  covered directly.

## Warning list

This is the short list of suites that are currently the least clearly
categorized.

### High-priority warnings

- Warning: `targets` mixes dry-run planner snapshots, real execution, and
  auto-update rewriting in one suite.
- Warning: `target_backend` mixes pure backend selection logic, warning
  rendering, dry-run planning, and command smoke coverage.
- Warning: `test_filter` mixes pure selector semantics with CLI resolution,
  dry-run graph filtering, update rewriting, and real execution.

### Medium-priority warnings

- Warning: `moon_commands` still contains pure output-generation coverage that
  is better suited to unit or snapshot tests than subprocess e2e tests.
- Warning: `diagnostics_format` mostly checks deterministic formatting and
  stream behavior, which is narrower than a full e2e suite.
- Warning: `warns` mixes dry-run planning checks with real warning-count and
  deny-warn behavior.
- Warning: `run_md_test` mixes markdown discovery/filter mapping with broad
  execution and update flows.
- Warning: `snapshot_testing` mixes rendering/normalization concerns with real
  backend execution and update behavior.

### Slow-path warnings

- Warning: `third_party`, `test_moonbitlang_x`, and network-dependent coverage
  tests should stay isolated from the default fast path.
- Warning: git-install and environment-dependent version tests should be
  treated as slow or external even if they remain under the main integration
  tree.

## Current audit

### Mostly unit-test candidates

These suites contain behavior that is mostly pure Rust logic and should live
next to the implementation:

- `diagnostics_format`
  - diagnostic string formatting and stream routing
  - still mostly lives in integration tests today
- `moon_commands`
  - shell completion generation and `add --help` checks
  - strong unit/snapshot candidates because the output is deterministic
- `test_filter`
  - name globbing, skip filtering, file/index filtering
  - strong unit-test candidates because the logic is mostly pure selection
- `target_backend`
  - backend support checks and package/backend matching are strong unit-test
    candidates in `crates/moon/src/filter.rs`
- `filter_by_path`
  - path canonicalization and package lookup are strong unit-test candidates in
    `crates/moon/src/filter.rs`

### Mostly snapshot-test candidates

These suites are dominated by deterministic planning output and should mostly be
graph or text snapshots, ideally below the CLI layer:

- `targets`
- `target_backend`
- `backend`
- `backend_config`
- `moon_build_package`
- `moon_bundle`
- dry-run parts of `warns`
- dry-run parts of `test_driver_dependencies`
- dry-run parts of `prebuild_link_config_self`
- parts of `specify_source_dir_001`

### Mostly assertion-only integration tests

These suites still need to spawn `moon`, but should avoid long output snapshots:

- `moon_commands` remaining stdin / heredoc / pipe tests
- `check_fmt`
- `clean`
- `query_symbol`
- `fuzzy_matching`
- `mbti`
- `output_format`
- `debug_flag_test`
- `dedup_diag`
- `native_stub_stability`

### True end-to-end suites

These suites exercise real toolchain or runtime behavior and should remain e2e,
though individual cases may still be split out:

- `moon_test`
- `snapshot_testing`
- `run_md_test`
- `run_doc_test`
- `moon_new`
- `blackbox`
- `inline_test`
- `value_tracing`
- `native_abort_trace`
- `wbtest_coverage`
- `prebuild`
- `prebuild_config_script`
- `virtual_pkg_test`
- `single_file_front_matter`

### Slow or external suites

These should not be mixed into the default fast path if they can be isolated:

- `third_party`
- `test_moonbitlang_x`
- network-dependent parts of `moon_coverage`
- git-install tests in `crates/moon/tests/test_cases/mod.rs`
- environment-dependent parts of `moon_version`

## Mixed suites that are not clearly separated yet

These are the main suites that currently combine multiple test kinds in one
module and should be split.

### `targets`

Currently mixes:

- e2e multi-target execution summaries
- dry-run graph snapshots
- auto-update file rewriting

Suggested split:

- planner and graph shape as snapshot tests below CLI
- one small e2e smoke test for multi-target execution
- one e2e suite for cross-target auto-update behavior

### `target_backend`

Currently mixes:

- backend selection and support checks
- warning rendering
- dry-run command planning
- `run` / `info` / `bundle` smoke coverage

Suggested split:

- backend matching and error construction as unit tests in `filter.rs`
- backend-orchestration tests starting from resolved project + explicit intent
- dry-run planner coverage as snapshot tests
- one or two assertion-only integration tests for command wiring only

### `test_filter`

Currently mixes:

- pure selector semantics
- path/package CLI resolution
- dry-run graph filtering
- auto-update rewriting
- real execution and parallelism

Suggested split:

- selector semantics as unit tests in `run::runtest::filter`
- path/package lookup as unit tests in `filter.rs`
- graph filtering as snapshot tests
- retain only path wiring, update, and real execution as e2e

### `moon_test`

Currently mixes:

- no-entry warnings
- async runtime behavior
- local dependency execution
- failure JSON
- patch-file behavior
- release/native backend behavior

Suggested split:

- warning formatting into unit or assertion-only integration tests
- keep async, patch, runtime, and backend execution as e2e

### `run_md_test`

Currently mixes:

- warning rendering
- failing execution output
- file/index filtering
- update flow
- generated metadata file checks

Suggested split:

- markdown discovery and filter mapping below CLI if possible
- metadata shape as snapshot tests
- keep one broad e2e for markdown test execution and update

### `snapshot_testing`

Currently mixes:

- failure rendering
- backend-specific execution
- update rewriting
- file snapshot content checks

Suggested split:

- failure rendering and normalization below CLI if possible
- keep one e2e failure case and one e2e update case

### `warns`

Currently mixes:

- dry-run argument propagation
- real warning counts
- deny-warn failure behavior

Suggested split:

- warning flag propagation as planner snapshots
- keep one e2e deny-warn failure and one warning-summary smoke test

### `moon_commands`

Currently mixes:

- pure text generation
- actual stdin execution

Suggested split:

- pure text generation as unit or snapshot tests
- keep stdin, heredoc, and pipe behavior as CLI process tests

## Recommended refactoring order

If the goal is to reduce default test time meaningfully, the best next targets
are:

1. `targets`
2. `target_backend`
3. `snapshot_testing`
4. `run_md_test`
5. `moon_test`

The first two are the best candidates because they contain a lot of dry-run and
selection coverage that should not need full CLI e2e tests.
