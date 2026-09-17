# 条件导入

编译和格式化这种语法还需要编译器和格式化工具支持条件导入块。

在 `moon.pkg` 中使用 `#cfg`，为不同后端选择导入：

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

每个后端只启用其中一个导入，因此两个实现可以使用相同的别名。
没有 `#cfg` 的导入块对所有后端生效。

支持的目标包括 `wasm`、`wasm-gc`、`js`、`native` 和 `llvm`。
可以使用 `all(...)`、`any(...)`、`not(...)` 组合条件，也可以使用
`true` 和 `false`。例如，`#cfg(any(target = "native", target = "js"))`
表示对 Native 和 JavaScript 生效。条件也适用于以 `for "test"` 或
`for "wbtest"` 结尾的测试导入块。

同一导入类型中，同一个包在每个后端只能启用一次，即使使用不同别名也不能重复。
条件互不重叠时可以重复导入；普通导入、黑盒测试导入和白盒测试导入之间也可以导入同一个包。

未启用的导入不会添加构建依赖。启用的跨模块导入仍需声明相应的模块依赖。
导入条件不会改变包的 `supported_targets` 设置。
