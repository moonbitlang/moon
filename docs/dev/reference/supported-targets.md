# `supported_targets` MVP Semantics

This page defines the current MVP semantics of `supported_targets` .

The MVP scope is target-aware package selection plus dependency-graph
compatibility checks for selected roots.

## Field definition

`supported_targets` is an optional field in both `moon.pkg.json` and
`moon.mod.json` .

Preferred syntax (expression string):

```json
{
  "supported-targets": "js"
}
```

Legacy syntax (array) is still accepted for compatibility:

```json
{
  "supported-targets": ["js", "native"]
}
```

Terms:

* `js`
* `wasm`
* `wasm-gc`
* `native`
* `llvm`
* `all`

Value semantics:

* omitted field: supports all backends, 
* expression string: start from empty set, then apply operations left-to-right
  ( `+` add, `-` remove); the first term may omit `+` , 
* legacy array: supports exactly the listed backends (deprecated).

Expression examples:

* `js`: support only `js`, 
* `all-js`: support all except `js`.

## Support rule

For selected backend `B` , package `P` is supported when:

1. `P.supported-targets` is omitted, or
2. `P.supported-targets` includes `B` after expression/list evaluation, 
3. and `M.supported-targets` (from containing module `M`) also includes `B`.

Effective package support is:

* `effective(P) = pkg_supported(P) ∩ mod_supported(M)`

Note: file-level conditional compilation `targets` (map keyed by filename) is a
separate feature and unchanged.

## Command behavior matrix

`B` means the backend selected for this invocation.

| Command | Broad mode (no explicit package/path filter) | Explicit package/path mode |
| --- | --- | --- |
| `moon check` , `moon build` | keep packages that support `B` before root selection | selected package must support `B` |
| `moon test` , `moon bench` | keep packages that support `B` | selected package(s) must support `B` |
| `moon run` | N/A (explicit selector required) | selected package must support `B` |
| `moon info` | write canonical `preferred-backend` output; inspect requested backend `B` | unsupported selected package(s) are skipped with warning |
| `moon bundle` | planner skips package targets that do not support `B` | no package-level explicit filter |

Notes:

* `--target all` expands to `wasm`,  `wasm-gc`,  `js`,  `native` (not `llvm`).
* `llvm` is still a valid value in `supported_targets`.
* `moon info` writes `pkg.generated.mbti` only from the canonical backend of each selected package: module `preferred-backend`, then workspace preferred backend, then `wasm-gc`.

## Dependency compatibility (fail-fast)

After root selection for backend `B` , Moon checks reachable required
dependencies from those roots. If any required dependency package does not
support `B` , the command fails with a normal user-facing error (not a panic).

## Warnings

* Legacy array syntax in `supported_targets` is deprecated; local packages using it emit a warning.
* If a selected root package omits `supported_targets` but depends on reachable
  packages that declare it, Moon emits a warning for that root.

## Out of MVP

MVP does **not** include:

* backend-scoped dependency declarations (`import` per backend), 
* transitive constraint propagation or inference (deduce `supported_targets` based on dependencies), 
* package-based backend guessing when `--target` is not provided.

## Mixed-backend usage pattern

Recommended package layout:

* backend-specific entry packages (`web`,  `server`) contain backend-specific deps, 
* shared package contains backend-neutral deps only.

Example:

```text
my_app/
  moon.mod.json
  src/
    shared/   # supported-targets: ["js", "native"]
    web/      # is-main: true, supported-targets: ["js"]
    server/   # is-main: true, supported-targets: ["native"]
```
