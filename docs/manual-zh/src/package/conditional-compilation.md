# 条件编译

条件编译的最小单位是文件。

在条件编译表达式中，支持三种逻辑操作符：`and`、`or` 和 `not`，其中 `or` 操作符可以省略不写。

例如，`["or", "wasm", "wasm-gc"]` 可以简写为 `["wasm", "wasm-gc"]`。

条件表达式中的条件可以分为后端和优化级别：

- **后端条件**：`"wasm"`、`"wasm-gc"` 和 `"js"`
- **优化等级条件**：`"debug"` 和 `"release"`

条件表达式支持嵌套。

如果一个文件未在 `"targets"` 中列出，它将默认在所有条件下编译。

示例：

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
