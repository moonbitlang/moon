# Conditional Compilation

Sometimes one may want to use different implementations on different platforms.
MoonBuild provides conditional compilation features for this case.

**All conditional compilation features are currently file-based.**
MoonBuild currently does not support conditional compilation on granularity less than one file.
Operating system-based conditional compilation is supported for native target platforms (native and llvm backends) through the `targets` field in `moon.pkg.json`.

## Filename-based conditional compilation

The extension of each MoonBit source code file (`.mbt`)
can be prefixed with a dot and then a platform target name (js, wasm, wasm-gc, native, llvm),
so that the file will only be included when compiling to the given target platform.
For example, `my_file.js.mbt` will only be included for building
when the target platform is JavaScript (`moon build --target js`).

Tests (`*_test.mbt`, `*_wbtest.mbt`) also accepts the same prefixes.
In this case, the prefix is located between the test suffixes and extension,
like `my_test.wasm.mbt` and `another_wbtest.native.mbt`.

If a file does not match any of the conditional compilation criteria above,
and it is not covered by configuration-based conditional compilation,
it will be always included in compilation.

## Configuration-based conditional compilation

The `targets` field in `moon.pkg.json` configures conditional compilation of source files
in additional to the filename-based approach.

The `targets` field is a map whose keys are the filenames,
and the values are the conditions where the condition is applied.
The condition is an expression written in JSON arrays, whose detail can be seen below.
Only files included in this map are considered for configuration-based conditional compilation.
Otherwise, it falls back to filename-based ones.

The `_test`/`_wbtest` magic suffixes on these files
still applies to the source file categorization,
regardless of the target platforms specified to the file.

### Conditional Expression Syntax and Semantics

Conditional expressions allow fine-grained control over when files are included in compilation based on target platform and optimization level. The expressions support logical operators and atoms (basic conditions).

#### Atoms

Atoms are the basic building blocks of conditional expressions:

**Target Platform Atoms:**

- `"js"` - JavaScript backend
- `"wasm"` - WebAssembly (MVP) backend
- `"wasm-gc"` - WebAssembly with garbage collection backend
- `"native"` - Native C backend
- `"llvm"` - LLVM backend

**Optimization Level Atoms:**

- `"release"` - Release (optimized) build
- `"debug"` - Debug (unoptimized) build

**Operating System Atoms (for native targets only):**

- `"windows"` - Windows operating system
- `"linux"` - Linux operating system
- `"macos"` - macOS operating system

Note: OS atoms only apply to native backends (`"native"` and `"llvm"`). For non-native backends (`"js"`, `"wasm"`, `"wasm-gc"`), OS atoms are ignored as these targets don't have an associated operating system.

#### Logical Operators

Conditional expressions support three logical operators:

**AND operator (`"and"`):**

- Evaluates to `true` only if all sub-expressions are `true`
- Syntax: `["and", expr1, expr2, ...]`
- Example: `["and", "js", "release"]` - Only include for JavaScript release builds

**OR operator (`"or"`):**

- Evaluates to `true` if any sub-expression is `true`
- Syntax: `["or", expr1, expr2, ...]`
- Example: `["or", "js", "wasm"]` - Include for either JavaScript or WebAssembly builds

**NOT operator (`"not"`):**

- Evaluates to `true` if none of the sub-expressions are `true` (equivalent to "all sub-expressions are false")
- Syntax: `["not", expr1, expr2, ...]`
- Example: `["not", "debug"]` - Include for all non-debug builds
- Example: `["not", "wasm", "wasm-gc"]` - Include for all targets except wasm and wasm-gc

#### Expression Formats

Conditional expressions can be specified in two formats:

**String format (single atom):**

```json
{
  "targets": {
    "file.mbt": "js"
  }
}
```

**Array format (complex expressions):**

```json
{
  "targets": {
    "file.mbt": ["and", "js", "release"]
  }
}
```

Note: Arrays can be nested to create complex expressions with multiple levels of logical operations.

#### Implicit OR operator

When an array starts with an atom (not a logical operator), it is treated as an implicit OR operation:

```json
{
  "targets": {
    "file.mbt": ["js", "wasm", "native"]
  }
}
```

This is equivalent to:

```json
{
  "targets": {
    "file.mbt": ["or", "js", "wasm", "native"]
  }
}
```

#### Examples

**Basic target selection:**

```json
{
  "targets": {
    "web_impl.mbt": "js",
    "wasm_impl.mbt": ["wasm", "wasm-gc"],
    "native_impl.mbt": "native"
  }
}
```

**Optimization-specific code:**

```json
{
  "targets": {
    "debug_helpers.mbt": "debug",
    "optimized_impl.mbt": ["and", "release", ["or", "js", "wasm-gc"]]
  }
}
```

**Excluding specific targets:**

```json
{
  "targets": {
    "fallback_impl.mbt": ["not", "native"],
    "non_wasm_impl.mbt": ["not", "wasm", "wasm-gc"]
  }
}
```

**Complex conditions:**

```json
{
  "targets": {
    "complex_impl.mbt": ["and", ["or", "js", "wasm-gc"], ["not", "debug"]]
  }
}
```

**OS-based conditional compilation (native targets only):**

```json
{
  "targets": {
    "windows_impl.mbt": "windows",
    "unix_impl.mbt": ["or", "linux", "macos"],
    "linux_specific.mbt": ["and", "native", "linux"],
    "non_windows.mbt": ["and", "llvm", ["not", "windows"]]
  }
}
```

Note: OS atoms (`"windows"`, `"linux"`, `"macos"`) only affect files when building with native backends (`"native"` and `"llvm"`). For non-native targets like JavaScript or WebAssembly, OS conditions are ignored since these targets don't run on a specific operating system.
