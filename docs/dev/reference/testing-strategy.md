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
- dry-run planner coverage as snapshot tests
- one or two assertion-only integration tests for command wiring

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
