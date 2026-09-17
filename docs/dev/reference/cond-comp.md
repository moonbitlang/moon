# Conditional Compilation

Sometimes one may want to use different implementations on different platforms.
MoonBuild provides conditional compilation features for this case.

MoonBuild supports conditional source-file selection and conditional package imports.
It does not select individual declarations within a source file; the compiler
handles source-level `#cfg` attributes. Import conditions do not currently
support architecture, operating system, optimization level, or custom cfg keys.

## Conditional package imports

The compiler and formatter also parse `moon.pkg`, so compiling or formatting
these manifests requires toolchain support for `#cfg` import blocks. Moon's
parser, dependency graphs, and dry-run planning support the syntax independently.

In `moon.pkg`, `#cfg` applies to the immediately following import block:

```moonbit
#cfg(target = "native")
import {
  "example/platform/native" @platform,
}

#cfg(not(target = "native"))
import {
  "example/platform/portable" @platform,
}

#cfg(any(target = "native", target = "js"))
import {
  "example/platform/test_helpers",
} for "test"
```

Conditions support `target = "wasm"`, `"wasm-gc"`, `"js"`, `"native"`, or
`"llvm"`, the constants `true` and `false`, and nested `all(...)`, `any(...)`,
and `not(...)`. `not` takes exactly one condition. `all()` is true and `any()`
is false. Each `#cfg(...)` takes exactly one condition; stacked attributes
are combined with AND. Unknown targets and unsupported expressions are errors.

Repeated import blocks are concatenated in declaration order. Conditions work
with regular imports, `for "test"`, and `for "wbtest"` (including their legacy
prefix forms). Unannotated blocks remain unconditional. Aliases, including
`*`, retain their existing meaning.

Within each import kind, a package may appear only once for any backend,
regardless of its alias. Package conversion rejects declarations whose backend
sets overlap before dependency graphs are constructed. Repeating a package with
disjoint conditions, or in different import kinds, is valid. Legacy import
options override the corresponding DSL blocks before this validation; both
`test_import`/`test-import` and `wbtest_import`/`wbtest-import` retain that behavior.

Package declarations retain each import's selected backend set. Module resolution
and package discovery run once. Command adapters then select the requested
backends, including module preferences when `--target` is omitted, before solving
package relationships. `ResolveOutput.pkg_rel` maps each requested backend to a
complete `DepRelationship`, including its import graph, virtual users,
implementation mappings, and transitive support.

Any requested backend's resolution error fails the command immediately. For
example, `moon check --target js,native` fails if JS resolution fails, even if
Native succeeds. `moon check --target native` does not resolve erroneous JS-only
imports. Errors are neither stored in `ResolveOutput` nor deferred to planning.
An inactive import does not resolve, create an alias, or contribute to cycle
checks or transitive backend support. Virtual implementation and override syntax
remains unconditional, with the resolved data owned by each backend's relationship.

Planning, lowering, and backend-scoped metadata consume the same resolved
relationship. Its derived support sets describe the intersection of active
dependencies' declared backends; they do not predict another backend's imports.
This applies across build, check, run, test, bench, info, doc, bundle, and proof
operations. `moon tree --package` requests all backends and shows the union of
their imports; any backend's resolution failure fails the command.

Conditions do not declare module dependencies or infer package
`supported_targets`. Imports from another module still require that module
to be declared in the module's dependencies. Only active import edges
participate in dependency compatibility checks.

Conditional imports are a `moon.pkg` feature. The parser retains its JSON-shaped
DSL representation; package conversion preserves import conditions in the package
model. Legacy `moon.pkg.json` imports remain unconditional.

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
