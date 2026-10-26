# 包配置

`moon.pkg` 支持直接声明以下包配置。表中的示例是独立配置。

| 声明 | 含义 |
| --- | --- |
| `experimental = true` | 标记实验性包，并向直接依赖它的包报告警告，默认值为 `false`。 |
| `proof_enabled = true` | 为包启用证明流程，默认值为 `false`。 |
| `max_concurrent_tests = 4` | 设置测试并发数，取值为 32 位无符号整数。 |
| `regex_backend = "table"` | 选择正则表达式编译后端，可选 `auto`、`block`、`table` 或 `runtime`。 |
| `virtual(has_default: true)` | 声明带默认实现的虚包；没有默认实现时使用 `false`。 |
| `virtual(implement: "user/module/interface")` | 实现指定的虚包。 |

这些声明与 `options(...)` 中对应配置的行为一致。`options(...)` 仍然支持
`proof-enabled`、`max-concurrent-tests`、`regex-backend` 及其下划线别名，
也支持以 `virtual_pkg` 作为 `virtual` 的旧别名；其中的 `implement` 对应
`virtual(implement: ...)`。

每个包最多声明一次 `virtual(...)`：包要么本身是虚包，要么实现一个虚包，
因此 `has_default` 和 `implement` 不能同时使用。

每项配置只需声明一次，直接声明或写在 `options(...)` 中均可。例如，不要同时使用
`proof_enabled = true` 和 `options("proof-enabled": true)`。

其他配置，包括 `bin-name`、`bin-target`、`overrides`、结构化的 `link`、
`native-stub` 和按文件配置的 `targets`，仍需写在 `options(...)` 中。

这些声明需要配套的编译器支持。不识别这些声明的编译器会报告
`Invalid configuration`（`E4192`）；使用这些工具链时，请继续使用旧的
`options(...)` 形式。格式化和 JSON 到 DSL 的迁移由工具链中的 `moonfmt`
提供。较旧的格式化工具可能会把直接声明改写成等价的 `options(...)` 形式。

## 实验性包

在包的 `moon.pkg` 顶层声明 `experimental = true` 后，直接依赖它的包会收到警告。
普通导入、白盒测试导入和黑盒测试导入均会检查，但只检查当前请求的后端启用的导入。
同一对包即使涉及多个测试目标或后端，也只报告一次警告。
该警告不会导致命令失败，`--quiet` 可以隐藏它。

此检查由 MoonBuild 执行。由于编译器也会读取 `moon.pkg`，实际编译时需要
编译器的配置解析器接受 `experimental` 字段；旧解析器可能报告
`Invalid configuration`（`E4192`）。

在使用方的 `moon.pkg` 中，可以按完整包名确认允许使用的实验性依赖：

```moonbit
import { "user/library/unstable" }
options("allow-experimental-packages": ["user/library/unstable"])
```

设置 `options("allow-experimental-packages": true)` 可以允许所有实验性依赖。
设为 `false`、空列表或省略该配置时，不豁免任何实验性依赖的警告。
列表项必须精确匹配完整包名，不匹配导入别名或包名前缀。
该配置只作用于声明它的包的直接依赖，不会传递到其他包。
稳定包仅依赖实验性包时，自身不会自动变成实验性包。

旧版 `moon.pkg.json` 支持顶层字段 `"experimental": true` 和
`"allow-experimental-packages": true`（或字符串列表）。

## 词法和语法分析器生成

Moon 不再根据 `.mbl` 或 `.mby` 文件自动运行 `moonlex` 或 `moonyacc`。
请显式声明生成 `.mbt` 文件的自定义规则。例如，假设 `my-lexer` 命令可用：

```moonbit
rule(name: "lexer", command: "my-lexer $input -o $output")
dev_build(rule: "lexer", input: "lexer.mbl", output: "lexer.mbt")
```

直接位于包目录中的 `.mbl` 或 `.mby` 文件必须声明为自定义规则的输入或输出，
旧的 `pre-build` 配置也可以使用。否则 Moon 会报告迁移错误。
发布包时应包含生成的 `.mbt` 文件，因为依赖该包的项目不会运行其包级预构建规则。
