# 包配置

`moon.pkg` 支持直接声明以下包配置。表中的示例是独立配置。

| 声明 | 含义 |
| --- | --- |
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
