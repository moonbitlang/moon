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

## 独立脚本

`moon build`、`moon check`、`moon test` 和 `moon run` 的显式 `.mbtx` 路径
会选择独立脚本，在项目内也一样。脚本使用 `moon.pkg` 语法在文件头声明包导入：

```moonbit
----
import {
  "moonbitlang/core/env",
}
----

fn main {
  println(@env.args())
}
```

脚本只导入显式声明的包。没有 import、空 `import {}` 或只有 `permission(...)`
声明时，包导入列表均为空。语言内建能力和常规 prelude 仍可用；引用 `@env` 等包
必须显式导入，关闭警告也不会允许未声明的 core 导入。版本路径、别名和 `*` 导入
仍受支持。相同规则也适用于 `moonx`、`moon run -`、`moon run -e` 和 `.mbtx`
预构建脚本。

对于 `moon check`、`moon run` 和 `moon test`，普通 `.mbt` 和 `.mbt.md`
路径用于选择所属包，需要项目上下文。
运行时编译整个包，并要求该包为可执行包；测试时选择指定文件中的测试。
项目外报告正常的项目查找错误，项目内找不到所属包时报告正常的包选择错误。

## 独立 Markdown

当 Markdown 文件顶部的 YAML 声明了 `moonbit.import` 时，
`moon check file.mbt.md` 和 `moon test file.mbt.md` 会将其作为独立文件处理，
显式空列表 `[]` 或空映射 `{}` 也一样。项目内外均可使用，只读取该文件及其声明的依赖，
不要求定义 `fn main`。

````markdown
---
moonbit:
  import:
    - path: moonbitlang/core/int
      alias: integer
  backend: js
---
```mbt check
test { assert_eq(@integer.MAX_VALUE, 2147483647) }
```
````

import 支持字符串列表、带 `path` 和可选 `alias` 的对象列表，以及路径到别名的映射。
外部模块必须至少在一个 import 中声明版本：`example/lib@1.2.3/first` 和
`example/lib/second` 可共用该版本。`example/lib@1.2.3` 导入模块根包。
同一模块的版本冲突会报错，`moonbitlang/core` 的导入不允许指定版本。
Markdown 保留编译器原有的标准库导入兼容行为，不使用 `.mbtx` 的严格导入限制。

后端优先级为命令行 `--target`、`moonbit.backend`、默认后端，支持多个命令行后端和
`--target all`。显式 `moon fmt file.mbt.md` 和 `moon check file.mbt.md --fmt`
也可在项目外使用。格式化只检查 YAML 语法，以及 header 和 `moonbit` 是否为映射；
只要存在 `moonbit.import` 字段，就按独立文件格式化，即使其值为 null 或尚未填写完成。
格式化不校验 import 值、导入路径、版本、backend 或 `deps`/`import` 互斥规则。
`moon check` 和 `moon test` 仍完整校验配置；`moon check --fmt` 会在格式化阶段之后进行完整校验。
默认构建产物和私有依赖隔离在 `_build/file.mbt.md` 下。

没有 `moonbit.import` 时，Markdown 继续选择所属包。仅有 `moonbit.deps` 不会启用独立模式，
也不会自动导入所有包。在 build、check、run 和 test 中，同时声明 `deps` 和 `import` 会报错；
非法 YAML、import 或 backend 会报告文件路径，不会回退到项目模式。
将独立 Markdown 传给 `moon build` 或 `moon run` 时，会明确提示改用
`moon check` 或 `moon test`。

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
