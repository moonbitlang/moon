# `moon bundle`

`moon bundle` consolidates all packages in a whole module into a single file. It is currently only used in `moonbitlang/core`, the standard library, to provide the many packages of the whole standard library within only one argument of the build commands.

The behavior of `moon bundle` is as follows:

1. Build all packages like `moon build`
2. Use `moonc bundle-core`, and include all non-virtual package cores in `core.core`.

Virtual packages are excluded from `core.core`, but their separately built
contract `.mi` and default implementation `.core` remain beside the bundle as
installed sidecars. An implementation-check `.impl.mi` is a local check output,
not a `moon bundle` product and not part of the installed bundle.

`moonbitlang/core/abort` is an ordinary package and is included in `core.core`.
Linking the installed standard library needs only `core.core`; older core
versions with a virtual `abort` package are no longer supported.
