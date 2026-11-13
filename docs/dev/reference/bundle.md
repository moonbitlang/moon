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

## Prebuilt Wasm library objects

With `MOON_SEPARATE_COMPILATION=1`, a release bundle for the linear-memory Wasm
backend also builds debug and release library objects for ordinary source
packages. Main packages and virtual packages do not produce bundled objects.
Both object profiles read the same release `.core` files distributed in the
bundle. Debug objects use `-g -O0`; release objects use the default optimization
and omit symbols. Their paths, relative to each package's bundle directory, are:

```text
wasm-objects/debug/<package>.o
wasm-objects/release/<package>.o
```

Toolchain packaging can opt in with the existing bundle command:

```sh
MOON_SEPARATE_COMPILATION=1 moon -C <toolchain-root>/lib/core bundle --all
```

Include the `wasm-objects` directories with the rest of
`lib/core/_build/wasm/release/bundle`. The compiler, Core IR, interfaces and
objects must come from the same toolchain build. This requires the compiler's
experimental Wasm library-object and `wasm-ld` support.

Objects are generated for the default WASI runtime, allocator and cycle-collection
settings. Debug-profile bundles, coverage-instrumented bundles and bundles with
other runtime settings keep producing only the existing `.core` and `.mi` files.

When separate compilation is enabled, builds using the injected standard library
reuse each existing object that matches their profile and symbol policy. Shipped
objects are tracked file inputs to the linker; they have no local compilation
action. Missing objects, older toolchains without objects, different runtime
settings, debug builds with `--strip`, and release builds with `--no-strip` use
local object compilation. This selection applies to build, run, test and bench;
the existing whole-program-link fallbacks are unchanged.
