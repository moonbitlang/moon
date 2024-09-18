# is-main 字段

`is-main` 字段用于指定一个包是否需要被链接成一个可执行的文件。

链接所生成的产物与后端相关，当该字段为 `true` 时：

- 对于 `wasm` 和 `wasm-gc` 后端，将会生成一个可以独立运行的 WebAssembly 模块。
- 对于 `js` 后端，将会生成一个可以独立运行的 JavaScript 文件。
