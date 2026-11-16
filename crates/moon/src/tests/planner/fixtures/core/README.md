# Core declarations for planning tests

This fixture supplies a fixed standard-library package graph to
`PlanningFixture::with_fixed_core`. It contains declarations only, with no
compilable implementation or prebuilt artifacts. Do not use it for E2E tests.

`prelude` imports `builtin` and `test`; `test` also imports `builtin`. This keeps
direct, transitive, and shared dependencies visible in the full graph snapshots
without making package-selection and profile tests depend on the installed
SDK's internal package graph. Standard-library artifacts are still modeled as
installed inputs, rather than local build actions.

The helper accepts a single module with no declared module dependencies and
skips module acquisition. It retains production package discovery, package
resolution, planning, and lowering. Compiler and runtime execution, module
acquisition, and real SDK compatibility remain the responsibility of their
existing integration and E2E tests.
