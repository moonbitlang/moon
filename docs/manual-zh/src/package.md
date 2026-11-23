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
`proof_enabled = true` 和 `options("proof-enabled": true)`，即使值相同也会报错。
`warnings` 与 `warn-list` / `warn_list`、`supported_targets` 与 `supported-targets`、
`dev_build` 与 `pre-build` / `pre_build` 同样遵循此规则，不会在两种形式之间合并或覆盖。

同一种类的多个 `import` 块仍按源码顺序合并，但不能同时使用 `options(...)` 中对应种类的
import 配置。空块和空的选项列表也算显式声明。普通、黑盒测试和白盒测试 import 相互独立。
仍可声明多个 `rule` 和 `dev_build`，其中规则名必须唯一。

每条 `dev_build` 或旧 `pre-build` 必须在 `command` 和 `rule` 中选择一项，
同时指定两项会报错，JSON 清单中的条目也遵循此规则。

在 `moon.mod` 中，每个模块依赖只能声明一次。`import` 内部，或 `import` 与
`options(deps: ...)` 之间的重复模块名都会报错，即使版本相同。
两种形式声明的不同依赖仍会合并。

其他配置，包括 `overrides`、结构化的 `link`、
`native-stub` 和按文件配置的 `targets`，仍需写在 `options(...)` 中。

这些声明需要配套的编译器支持。不识别这些声明的编译器会报告
`Invalid configuration`（`E4192`）；使用这些工具链时，请继续使用旧的
`options(...)` 形式。格式化和 JSON 到 DSL 的迁移由工具链中的 `moonfmt`
提供。较旧的格式化工具可能会把直接声明改写成等价的 `options(...)` 形式。

## 配置诊断

`options(...)`、JSON 清单及嵌套配置中的未知键会产生警告。例如，
`link.native.cc_link_flags` 会触发警告，因为正确拼写是 `cc-link-flags`。
警告不会中止命令，可通过 `--quiet` 隐藏；依赖中的未知键警告默认不显示。
此规则同时适用于模块和包配置。

已移除的模块字段 `checksum` 和包字段 `bin-name` / `bin-target` 遵循上述未知键策略。
当前项目和 workspace 成员中的模块字段 `include` 和 `exclude` 则会报错，即使值为空，
因为忽略旧过滤条件可能把非预期文件打包。请将过滤条件迁移到 `.gitignore` 或 `.moonignore`；
同目录下 `.moonignore` 优先于 `.gitignore`。依赖中的这两个旧字段会被忽略。

已移除的包字段 `name` 遵循上述未知键策略，包括 `name: "main"`。
请使用 `pkgtype(kind: "executable")` 声明可执行包。模块 `scripts` 仅识别
由 `moon run-postadd` 显式执行的旧 `postadd` 配置。其他名称，例如 `prebuild`
和 `postbuild`，会作为未知键警告，不会注册构建钩子。

在 `moon.mod` 中，`warnings` 和 `warn-list`、`preferred_target` 和 `preferred-target`，
或 `supported_targets` 和 `supported-targets`，不能同时声明。
即使值相同或为空，或两种拼写都出现在 `options(...)` 中，也会报错。

已知字段的类型错误会报错，包括依赖中的配置以及使用 `--quiet` 时。
例如，`warnings = 123` 无效，因为 `warnings` 必须是字符串。
由编译器消费的配置，例如 `options("alert-list": "+all")`，也由 Moon 识别并检查类型。

已弃用的包配置也会在当前项目和 workspace 成员中产生警告，依赖中的警告默认抑制。
请将 `is-main: true` 替换为 `pkgtype(kind: "executable")`，将库包的布尔
`link: true` 替换为 `pkgtype(kind: "foreign_library")`。值为 false 的旧标志，
以及可执行包的布尔 `link`，直接删除即可。结构化 `link` 仍是后端配置。
`link.*.exports` 已弃用，请改用源码中的 `#export_name` 属性；空导出列表也会警告。
需要产出供外部宿主使用的库文件时，请声明 `pkgtype(kind: "foreign_library")`。
这些旧字段将在后续移除。

模块 prebuild 脚本还会收到协议迁移提示：旧 stdin JSON 输入即将移除，
请改为读取 `MOON_MOD`、`MOON_BUILD_DIR` 等 `MOON_*` 环境变量。
stdout JSON 继续作为结果协议。该提示仅针对当前项目和 workspace 成员，
即使当前后端不运行脚本也会显示；它不会检测脚本是否读取 stdin。
`--quiet` 会抑制上述弃用警告。

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
