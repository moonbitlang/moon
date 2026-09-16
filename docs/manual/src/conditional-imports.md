# Conditional imports

Compiling and formatting this syntax also requires a compiler and formatter
that support conditional import blocks.

Use `#cfg` in `moon.pkg` to select imports for a backend:

```moonbit
#cfg(target = "native")
import {
  "example/platform/native" @platform,
}

#cfg(not(target = "native"))
import {
  "example/platform/portable" @platform,
}
```

Both implementations can use the same alias because only one import is active
for each backend. Unannotated import blocks apply to every backend.

Supported targets are `wasm`, `wasm-gc`, `js`, `native`, and `llvm`. Combine
conditions with `all(...)`, `any(...)`, and `not(...)`, or use `true` and
`false`. For example, `#cfg(any(target = "native", target = "js"))` selects
Native and JavaScript. Conditions also apply to import blocks ending in
`for "test"` or `for "wbtest"`.

Inactive imports do not add build dependencies. An active import from another
module still requires a module dependency declaration. Import conditions do
not change the package's `supported_targets` setting.
