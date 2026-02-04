# Rupes Recta Special Cases

Rupes Recta keeps a growing list of hard-coded behaviors so the new build
pipeline remains byte-for-byte compatible with the legacy engine. The handlers
live throughout `crates/moonbuild-rupes-recta`, but this page summarizes the
important ones and why they exist.

## Discovery-time injections

- **Record `moonbitlang/core/abort` when present.** During discovery, if
  `moonbitlang/core/abort` exists in the resolved package set, we record its
  package ID via `DiscoverResult::set_abort_pkg`. This preserves the legacy
  override behavior without synthesizing a separate package.
- **Merge coverage sources into builtin.** `inject_core_coverage_into_builtin`
  copies `moonbitlang/core/coverage` sources into `moonbitlang/core/builtin` so
  downstream compilation steps see a builtin package that already contains the
  coverage helpers. This is essential for `builtin` to correctly get code
  coverage support. This is **only available when `--enable-coverage` is set**.
- **Pin prelude imports for core packages.** Whenever discovery encounters a
  package under the core module, `add_prelude_as_import_for_core` injects
  `moonbitlang/core/prelude` into `test_imports`. Without this implicit import,
  core tests would fail to compile because they rely on the prelude symbols.

## Solver-time dependency hacks

- **Core packages auto-link coverage.** `pkg_solve::inject_core_coverage_usage`
  wires every non-exempt core package to `moonbitlang/core/coverage` (skipping
  the coverage and builtin packages themselves, plus libraries marked to skip
  coverage entirely). The dependency is added for Source/Inline/Whitebox test
  targets so later phases don’t need to special-case coverage instrumentation.

## Build/test filtering logic

- **Tests can be dropped per package.** `special_cases::should_skip_tests` lists
  packages that should not produce test targets (currently just
  `moonbitlang/core/abort`). `compile::filter_special_case_input_nodes` uses
  this to discard matching test build nodes before planning.
- **Coverage rules differ per package.** `should_skip_coverage` ensures abort is
  never instrumented, while `is_self_coverage_lib` says builtin/coverage should
  use themselves when linking coverage support. These predicates are reused by
  the solver and later lowering stages.
- **Builtin detection helpers.** `is_builtin_lib` centralizes the logic for
  spotting `moonbitlang/core/builtin`, allowing downstream code to keep a single
  source of truth whenever builtin needs override behavior.

## Artifact path overrides

- **Abort artifacts live beside the stdlib.** When lowering build plans
  (`build_lower::artifact`), calls that would normally resolve `.core`, `.mi`,
  or `.phony_mi` files check whether the target package is the recorded abort
  package. If so, the code switches to the stdlib’s prebuilt `abort` outputs
  (`abort_core_path` / `abort_mi_path`) because those artifacts are shipped as
  part of the toolchain rather than being rebuilt per project.

## Runtime + tooling side effects

- **Skip abort during coverage/test runs.** `special_cases::should_skip_tests`
  and `should_skip_coverage` prevent abort from producing test targets or being
  instrumented for coverage.
- **Core coverage is folded into builtin artifacts.** Once coverage sources are
  merged into builtin and coverage edges are injected, the rest of the pipeline
  can assume builtin already carries every helper needed for coverage-enabled
  builds.

If you need to add another compatibility shim, keep the behavior in the module
nearest to the code it affects, but document the motivation here so future work
can determine whether the special case is still necessary.
