# `moon bundle`

`moon bundle` builds the per-package artifacts used to distribute a module. Its
primary use is preparing `moonbitlang/core`, the standard library, for toolchain
packaging.

The behavior of `moon bundle` is as follows:

1. Select every package with an implementation that supports the requested
   backend, including packages that no other package imports.
2. Build each selected package's `.core` and `.mi` files in
   `_build/<backend>/release/bundle`, preserving the package directory layout.

Virtual packages with default implementations produce their contract `.mi` and
default implementation `.core`. Contracts of pure virtual packages are built
when required by a selected package.

Linking against the installed standard library uses the reachable per-package
`.core` files in dependency order.
