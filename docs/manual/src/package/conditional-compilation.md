# Conditional Compilation

The smallest unit of conditional compilation is a file.

In a conditional compilation expression, three logical operators are supported: `and`, `or`, and `not`, where the `or` operator can be omitted.

For example, `["or", "wasm", "wasm-gc"]` can be simplified to `["wasm", "wasm-gc"]`.

Conditions in the expression can be categorized into backends and optimization levels:

- **Backend conditions**: `"wasm"`, `"wasm-gc"`, and `"js"`
- **Optimization level conditions**: `"debug"` and `"release"`

Conditional expressions support nesting.

If a file is not listed in `"targets"`, it will be compiled under all conditions by default.

Example:

```json
{
    "targets": {
        "only_js.mbt": ["js"],
        "only_wasm.mbt": ["wasm"],
        "only_wasm_gc.mbt": ["wasm-gc"],
        "all_wasm.mbt": ["wasm", "wasm-gc"],
        "not_js.mbt": ["not", "js"],
        "only_debug.mbt": ["debug"],
        "js_and_release.mbt": ["and", ["js"], ["release"]],
        "js_only_test.mbt": ["js"],
        "js_or_wasm.mbt": ["js", "wasm"],
        "wasm_release_or_js_debug.mbt": ["or", ["and", "wasm", "release"], ["and", "js", "debug"]]
    }
}
```
