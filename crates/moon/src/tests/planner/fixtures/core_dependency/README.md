# Core as an installed dependency

This fixture supplies a fixed standard-library package graph to
`PlanningFixture::with_core_dependency`. It contains declarations only, with no
compilable implementation or prebuilt artifacts. Do not use it for E2E tests.

Only `builtin` and `prelude` are needed; prelude imports builtin.
`core_dependency.rs` checks the core-specific implicit aliases, installed
artifact paths, and absence of local core compilation for an ordinary project.
These rules do not vary by backend, so the scenarios select Wasm GC explicitly.
Generic dependency closures and deduplication belong to the lower-layer tests,
not to this core fixture.

Package-selection and profile scenarios also use this fixture as their
environment. Their assertions concern the ordinary project's targets, paths,
and compiler options, not core's internal package graph.

The helper accepts a single non-core module with no declared module dependencies
and skips module acquisition. It retains production package discovery, package
resolution, planning, and lowering. Compiler and runtime execution, module
acquisition, and real SDK compatibility remain the responsibility of their
existing integration and E2E tests. These scenarios intentionally do not track
the installed SDK's exact package composition.

Do not use this fixture as a core project. That role must go through module
synchronization so tests can detect accidental injection of a second core.
See [core_project](../../../../../tests/test_cases/core_project/README.md) for
local core discovery rules and [the fixture map](../../../../../../../docs/dev/testing.md#core-fixture-roles)
for the existing backend and IDE scenarios.
