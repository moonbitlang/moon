# Core as an installed dependency

This fixture supplies a fixed standard-library package graph to
`PlanningFixture::with_core_dependency`. It contains declarations only, with no
compilable implementation or prebuilt artifacts. Do not use it for E2E tests.

`prelude` imports `builtin` and `test`; `test` also imports `builtin`. `unused` is
unreachable. This keeps direct, transitive, and shared dependencies visible in
the full graph snapshots
without making package-selection and profile tests depend on the installed
SDK's internal package graph. Standard-library artifacts are still modeled as
installed inputs, rather than local build actions. `core_dependency.rs` checks
the implicit import aliases, exact reachable artifact inputs, and absence of
core compilation for an ordinary project on Wasm GC and JavaScript.

The helper accepts a single non-core module with no declared module dependencies
and skips module acquisition. It retains production package discovery, package
resolution, planning, and lowering. Compiler and runtime execution, module
acquisition, and real SDK compatibility remain the responsibility of their
existing integration and E2E tests. These snapshots intentionally do not track
the installed SDK's exact package composition.

Do not use this fixture as a core project. That role must go through module
synchronization so tests can detect accidental injection of a second core.
See [core_project](../../../../../tests/test_cases/core_project/README.md) for
local core discovery rules and [the fixture map](../../../../../../../docs/dev/testing.md#core-fixture-roles)
for the existing backend and IDE scenarios.
