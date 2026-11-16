# Core as a project

This fixture models `moonbitlang/core` as the root module for package discovery
and package relationship resolution, followed by build planning. It is not
compiled. Its `builtin` and `coverage` source files make the root-only
coverage-to-builtin merge observable;
the empty package manifests show that those packages need no self-imports.

The `probe` package includes a blackbox-test source so tests can assert that
core's automatic `prelude` import applies to blackbox-test targets without
adding an implicit import to source targets. Keep this separate from the fixed
core declaration fixture used by ordinary projects and from the IDE symbol
query fixture.

The build graph must produce local builtin/prelude interfaces and IR instead of
reading installed core artifacts. Source compile commands must not receive the
implicit core imports that ordinary projects receive.
