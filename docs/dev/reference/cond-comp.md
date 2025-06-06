# Conditional Compilation

Sometimes one may want to use different implementations on different platforms.
MoonBuild provides conditional compilation features for this case.

**All conditional compilation features are currently file-based.**
MoonBuild currently does not support conditional compilation on granularity less than one file.
It also does not support that based on the architecture or operating system of native target platforms.

## Filename-based conditional compilation

The extension of each MoonBit source code file (`.mbt`)
can be prefixed with a dot and then a platform target name (js, wasm, wasm-gc, native, llvm),
so that the file will only be included when compiling to the given target platform.
For example, `my_file.js.mbt` will only be included for building
when the target platform is JavaScript (`moon build --target js`).

Tests (`*_test.mbt`, `*_wbtest.mbt`) also accepts the same prefixes.
In this case, the prefix is located between the test suffixes and extension,
like `my_test.wasm.mbt` and `another_wbtest.native.mbt`.

## Configuration-based conditional compilation

The `targets` field in `moon.pkg.json` configures conditional compilation of source files
in additional to the filename-based approach.

The `targets` field is a map whose keys are the filenames,
and the values are the conditions where the condition is applied.
The condition may either be:

- An array of platform target names.
  The file will be included iff compiled with the given platform targets.
  e.g. `["native"]`, `["js", "wasm-gc"]`.
- An array with `not` as the first element, and platform target names in the rest.
  The file will be included iff **not** compiled with the given platform targets.
  e.g. `["not", "wasm", "wasm-gc"]`.

The `_test`/`_wbtest` magic suffixes on these files
still applies to the source file categorization,
regardless of the target platforms specified to the file.
