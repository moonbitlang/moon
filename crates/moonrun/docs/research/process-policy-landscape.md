# Moonrun process policy 调研

> 状态：调研稿，不是 ADR 或已接受设计<br>
> 调研日期：2026-08-18<br>
> 范围：process spawn admission、交互 approval 与 child sandbox；重点考察 prefix rule
> 实现注记：首版已选择不改 Windows guest ABI 的 best-effort requested-command guardrail。

## 摘要

如果 Moonrun 要增加 prefix rule，最稳妥的方向是把它定义为**静态 spawn
admission 规则**，而不是字符串前缀，也不是一次 approval 的持久化形式：

1. 规则语义匹配逻辑 `program + args`；`args_prefix` 按 token 精确匹配。Unix 直接检查
   结构化 job；Windows 按 MoonBit 的同一套规范编码规则生成 command-line prefix。
2. Moonrun v1 宜只配置 allow entries：命中即 allow，未命中即 deny；不要把
   `ask` 放进静态 policy。若未来由 harness 提供审批，应把一次性 approval 作为独立层。
3. 允许启动某个 executable 不等于给它或其后代加了沙箱。OS child sandbox
   必须是第三个独立层。
4. v1 不需要 deny rule 或规则顺序；未来若引入 managed/user 多层 policy，所有命中
   应取最严格结果，managed deny 不能被低层配置或临时批准放宽。
5. shell、解释器、脚本、环境变量、`PATH`/symlink 解析是 prefix 规则最主要的
   绕过面。因此首版只承诺 requested-command guardrail，不承诺 executable identity。
6. Moonrun 现有 Unix job 已保留结构化 `path + argv`，Windows job 却只有规范编码后的
   command line。首版在 Windows 做带 token 边界的 canonical prefix 比较；未来若要
   强身份保证，应另加 resolved/absolute program 模式，而不是暗中改变普通 spawn 语义。

这三个层次不能合并：

| 层 | 回答的问题 | 推荐状态 |
|---|---|---|
| 静态 spawn admission | 这个逻辑 process 请求是否符合配置？ | Moonrun v1：`allow / deny`；通用求值器可保留 `unmatched` |
| 交互 approval | 对一次未命中或越界请求，是否临时授权？ | `allowed-once / rejected / unavailable` |
| OS child sandbox | 已获准的进程及后代实际能读、写、联网或再 spawn 什么？ | 独立 capability/profile |

Codex 展示了较完整的 token-prefix 规则与复合 shell 处理；Claude Code 展示了
更多 shell 规范化和 managed precedence；OpenCode 展示了易配置的 glob 和
session approval，但也暴露出字符串匹配及顺序优先级的风险；DeepSeek Harness
最值得借鉴的是把 approval、sandbox、shell 和 subprocess 拆为独立 seam；Deno
则清楚证明 scoped executable allowlist 仍不是 child sandbox。

## 对照表

| 系统 | 静态规则模型 | 匹配与优先级 | 复合 shell / 间接执行 | 临时或持久批准 | enforcement / audit |
|---|---|---|---|---|---|
| Codex | `prefix_rule(pattern, decision)`；`allow / prompt / forbidden` | 结构化 token prefix；同一位置可列候选 token；所有命中取最严格 `forbidden > prompt > allow` | 简单 shell 链拆成 segment；复杂 shell 语法退回匹配整个 wrapper argv | 支持单次、session、写入 exec policy amendment | approval 与 OS sandbox 分离；规则可带正反例，`execpolicy check` 显示匹配结果；可选 OTel decision event |
| OpenCode | tool permission + resource glob；`allow / ask / deny` | 字符串 glob，最后一个命中规则生效；V2 多 resource 再取最严格 | 产品文档称 Bash 会拆命令；V2 dev 源码仍以整条 command string 请求权限并标 TODO | `once / always / reject`；`always` 保存建议 pattern | Bash 在 host 上运行，不是 OS sandbox；有 permission asked/replied 事件，但未见同等成熟的 rule-check 诊断 |
| Claude Code | `Tool(specifier)`；`allow / ask / deny` | deny、ask、allow 分阶段求值；managed deny 不可覆盖 | 拆 shell operator；规范化有限 wrapper/env；对 `xargs`、`find -exec`、env runner 等单列防护 | 可记住 project/command 规则；静态 deny/ask 不能被 hook 放宽 | tool permission 与 sandbox 分离；client enforcement；PreToolUse hook 只能进一步收紧 |
| DeepSeek Harness | 通用 `pre-execute` gate 有 `allow / deny / ask`，但没有内置 prefix rule | approval request 本身不含 tool args，不能做 argv matcher | Bash 是单个 `bash -c` 字符串；sandbox denial 后可一次性重试提升 | 只有 `allowed-once`；无 `allow-always`、grant store、revocation | approval、file-effects sandbox、shell、subprocess 独立；事件化并 fail closed |
| Deno | `--allow-run=<PROGRAM_NAME>` / `--deny-run` | 只 scope executable，不约束 argv | 允许 shell/解释器即可越过 Deno runtime 权限；`LD_*` / `DYLD_*` 要求 unscoped allow | TTY 可请求 runtime permission；非 TTY 或 `--no-prompt` 不询问 | Deno permission 只守 parent runtime API；spawned child 不在 Deno sandbox 中 |

## 1. Codex：token prefix 是可借鉴的基线

Codex 的 [Rules 官方文档](https://developers.openai.com/codex/rules/)
把规则定义为：

```python
prefix_rule(
    pattern = ["cargo", "test"],
    decision = "allow",
    justification = "Run Rust tests",
    match = [["cargo", "test", "-p", "example"]],
    not_match = [["cargo", "metadata"]],
)
```

关键性质：

- `pattern` 匹配 argv 开头的**有序 token**，不是 command string 的字符前缀；
  某一 token 位置可以用候选集合表达有限 alternatives。
- 多条规则同时匹配时采用最严格结果：`forbidden > prompt > allow`。这允许用
  宽一些的 allow 加更具体的 deny/prompt carve-out，而不会因文件顺序静默放宽。
- `allow` 是一个 sandbox exception：命令可在 sandbox 外执行且不再询问。因此
  prefix rule 的安全含义远大于普通 UI 偏好。
- 规则文件可内置 `match` / `not_match` 示例，加载时即校验；官方
  [`execpolicy` README](https://github.com/openai/codex/blob/main/codex-rs/execpolicy/README.md)
  还提供 `check` 命令，输出命中规则和最终决定。

### executable identity

[`host_executable`](https://github.com/openai/codex/blob/main/codex-rs/execpolicy/README.md)
可登记 executable 名字和可信绝对路径。matcher 先匹配调用中的第一个 token；
必要时可按绝对路径 basename 回退。若为该名称配置了 `host_executable`，只有列出的
绝对路径能使用回退。

这个机制点出了一个重要问题：`["git", "status"]` 并未天然说明实际执行的是哪一个
`git`。`PATH`、cwd、symlink 和可写 executable 目录都会改变程序身份。Moonrun
不能只保存用户输入的 basename；至少要审计 resolver 最终选中的路径，并避免从
guest 可写目录解析可信 executable。

### compound shell

对 `bash|sh|zsh -c/-lc`，Codex 会尝试解析 shell script。由普通 word 及
`&&`、`||`、`;`、`|` 组成的线性命令会被拆成独立 segment，每段分别求值，再取
最严格结果。例如 `git status && cargo test` 不能因为第一个 segment 被允许就放行
整条命令。

若 script 含 redirection、command substitution、env assignment、glob 或控制流等
高级语法，Codex 不再拆分，而是匹配整个 shell wrapper argv。官方
[approval-policy prompt](https://github.com/openai/codex/blob/main/codex-rs/prompts/templates/permissions/approval_policy/on_request.md)
同时要求不要为裸 `python3` 等通用解释器建议 prefix，也不要为 destructive `rm`
或 heredoc/herestring 生成持久规则。

这里仍有边界：只要显式允许一个足够宽的 `bash -c`、解释器或 runner prefix，内部
代码就会成为 opaque payload。对 Moonrun，更安全的 v1 是对这类 opaque request
返回 `unmatched`/`deny`，而不是允许整 wrapper 的宽 prefix。

### approval、sandbox 与审计

Codex 的 [Approvals & security](https://developers.openai.com/codex/agent-approvals-security/)
明确区分 OS sandbox 与 approval routing。App-server 协议还区分单次接受、session
接受以及带 exec-policy amendment 的接受；参见
[app-server README](https://github.com/openai/codex/blob/main/codex-rs/app-server/README.md)
和 [`ExecPolicyAmendment`](https://github.com/openai/codex/blob/main/codex-rs/protocol/src/approvals.rs)。
可选 OpenTelemetry 会记录 tool decision、批准/拒绝和 decision source，但官方也
提醒 command arguments/results 可能敏感，审计默认并非自动安全。

## 2. OpenCode：易用 glob 的收益与代价

[OpenCode permissions 文档](https://opencode.ai/docs/permissions/) 使用
`allow / ask / deny`，既可按 tool 设置，也可为 Bash 等 tool 配置 resource pattern：

```json
{
  "permission": {
    "bash": {
      "*": "ask",
      "git status": "allow",
      "git log *": "allow",
      "rm *": "deny"
    }
  }
}
```

规则是简单字符 glob：`*` 匹配任意字符、`?` 匹配单个字符；最后一个匹配项生效。
优点是配置紧凑，也容易表达 path pattern；缺点是安全性依赖空格和字符串表示，
不像 argv-token matcher 有稳定边界，而且重排或 merge 配置可能改变结果。`git *`
只是在字符层面匹配，不等于理解 `git` 的 flag、alias、config injection 或 subcommand
语义。

产品文档当前对多数 tool 的默认值较宽松；`external_directory`、重复调用检测等默认
询问，`.env` 文件有额外 deny。Moonrun 若把 process 视为 host-capability boundary，
不应继承这种面向 coding-agent UX 的默认 permissive posture。

OpenCode 文档说 Bash permission 会对 parsed command 分段，并支持
`once / always / reject`；`always` 可在当前 session 接受 tool 建议的 pattern。
per-agent permission 与全局配置合并，agent-specific 配置优先。

不过，官方 `dev` 分支的 V2 实现处于迁移中：

- [`permission.ts`](https://github.com/anomalyco/opencode/blob/dev/packages/core/src/permission.ts)
  用 ordered rules 和 `findLast` 求值；configured deny 会在 saved approval 之前检查，
  因此临时批准不能覆盖显式 deny；一次调用涉及多个 resource 时取最严格结果。
- [`wildcard.ts`](https://github.com/anomalyco/opencode/blob/dev/packages/core/src/util/wildcard.ts)
  把 glob 编译为覆盖完整字符串的 regex；Windows 下大小写不敏感。
- [`bash.ts`](https://github.com/anomalyco/opencode/blob/dev/packages/core/src/tool/bash.ts)
  当前把整条 `input.command` 作为 permission resource，并明确留下 tree-sitter parser、
  reusable prefix approval 和 parser-based external-path detection 的 TODO。shell 默认在
  host user 权限下执行，并没有 child OS sandbox。

因此 OpenCode 可以作为配置 UX 参考，但不宜把“字符串 glob + last match wins”直接
作为 Moonrun 的安全核心。稳定文档和 V2 dev source 在 command parsing / approval
persistence 上存在版本差异，设计时不能混成一个已稳定契约。

## 3. Claude Code：shell 规范化与不可放宽的 managed policy

Claude Code 没有公开实现源码，本节只依据其
[Permissions 官方文档](https://code.claude.com/docs/en/permissions)。它的规则形如
`Bash(git status *)`，也有 `allow / ask / deny`，但求值顺序是先 deny、再 ask、最后
allow；managed deny 以及更高层约束不能被低层 allow 覆盖。project allow 只在用户
信任 workspace 后生效。

对 Bash，Claude Code：

- 解析 `&&`、`||`、`;`、pipe、后台操作和 newline，各 subcommand 都必须通过；
- 只剥离一组固定 wrapper，如 `timeout`、`time`、`nice`、`nohup`、`command`；
- 对 env assignment、`xargs`、环境 runner、`find -exec/-delete`、PowerShell AST 和
  alias canonicalization 有单独规则；
- 明确警告 argument-constrained pattern 容易出错，不能靠字符串 pattern 表达所有
  URL、flag 或 shell 语义。

`PreToolUse` hook 能将请求改为 deny/ask/allow，但不能绕过静态 deny/ask；blocking
hook 仍能覆盖 allow。这种“扩展点只能收紧、不能放宽”与 Codex/DeepSeek 的
monotonic guard 一致，适合 managed policy。

Claude Code 也把 tool permission 与 OS sandbox 分开：sandbox 约束 Bash child 的
filesystem/network；permission 决定 tool request 是否可发起。即使 sandbox 允许自动
批准一般 Bash，content-specific deny/ask 和关键路径检查仍继续生效。

## 4. DeepSeek Harness：分层 seam，而不是 prefix 参考实现

截至调研日期，明确的一方项目是
[deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)。README
声明由 DeepSeek AI 开发，但项目仍处于 developer preview，会发生 breaking change。

DeepSeek Harness 当前**没有 prefix rule，也没有持久命令授权**：

- [approval subsystem](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/subsystems/approval.md)
  的 session policy 只有 `ask | never`，outcome 只有
  `allowed-once | rejected | cancelled | unavailable`；缺省或异常 fail closed。
- [user-approval package](https://github.com/deepseek-ai/deepseek-harness/tree/master/packages/interaction/user-approval)
  明确将 `allow-always`、remembered rule、revocation 和 grant store 列为未实现。
  `ApprovalRequest` 刻意不带 tool arguments，只带 `toolName`、`callId`、`reason` 等，
  UI 通过 `callId` 关联之前展示的调用。因此 approval seam 自身不能审查 argv/prefix。
- [tool-bash](https://github.com/deepseek-ai/deepseek-harness/tree/master/packages/shell/tool-bash)
  把 command string 交给 `bash -c`。真实 sandbox denial 后，模型可携带 justification
  重试较高 sandbox mode；批准只影响该 call。
- [sandbox-policy](https://github.com/deepseek-ai/deepseek-harness/tree/master/packages/sandbox/sandbox-policy)
  只描述 `read-only | workspace-write | danger-full-access` 文件效果；官方明确将
  network、process policy 和 process visibility 排除在 vocabulary 外。
- [subprocess package](https://github.com/deepseek-ai/deepseek-harness/tree/master/packages/subprocess)
  只管 executable lookup、process tree、stdio、termination 和 cleanup；命令语义归
  上层 consumer。
- 通用 [tools pre-execute pipeline](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/core/tools/README.md)
  能看到冻结后的 tool args，并提供 `allow / deny / ask` waterfall 与不可被后续
  listener 放宽的 monotonic guard。理论上 prefix policy 应放在这里，但官方尚未
  实现这种配置。

[Permission presets](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/subsystems/permission-presets.md)
只是组合两个独立 knob，例如 `workspace-write = workspace-write sandbox + ask approval`。
这给 Moonrun 的直接启示是：prefix matcher 必须放在仍看得到结构化 arguments 的
pre-execute 层，不能塞进 approval responder；durable policy 与 one-shot grant 也应
分开存储和审计。

## 5. Deno：scoped `--allow-run` 不是 child sandbox

Deno 的 [Permissions reference](https://docs.deno.com/runtime/reference/permissions/)
允许用 `--allow-run=git,curl` scope 可启动的 program，并用 `--deny-run` carve out。
这相当于一个只匹配 executable identity、不匹配 argv 的静态 spawn allowlist。

官方同时明确说明：spawned subprocess 独立于 parent Deno permissions，可以访问
parent 无权访问的 host resource。因此 `--allow-run=deno` 尤其危险，child 可自行带
`--allow-all`；shell 和通用解释器具有同样的间接执行问题。
[Security 文档](https://docs.deno.com/runtime/fundamentals/security/) 建议只允许特定可信
executable，并避免允许 shell/deno；若确需执行不可信代码，还应使用 OS sandbox、VM
或 microVM。

Deno 还提供两个对 Moonrun 很有价值的绕过案例：

1. 若 guest 可写入 allowed executable 本身或其所在目录，它可以替换 binary，再以
   host 权限启动攻击者代码。因此 executable path 的可信度与 file policy 关联，但
   不能仅靠 basename 规则解决。
2. scoped `--allow-run=echo` 不允许 child environment 携带 `LD_*` 或 `DYLD_*`；这些
   变量能让 dynamic linker 注入任意 library，Deno 因而要求 unscoped `--allow-run`，
   等于显式承认 scoped executable 保证已失效。

Deno 的 interactive permission prompt 在非 TTY 或 `--no-prompt` 下不会出现；其
permission broker 则接管所有 decision 并在连接、消息序号或协议异常时立即终止。
这支持 Moonrun 在 headless 场景将无法 approval 视为 deny，而不是隐式 allow。

## 跨实现结论

### 1. prefix 应是 argv token prefix，不是字符 prefix

规则应匹配结构化 spawn request：

```text
resolved_executable + argv[] + cwd + env_delta + execution_kind
```

v1 的 `args_prefix` 宜只支持 exact token；有限 alternatives 可先用多条 allow entry
表达。不要直接支持 regex、任意 `*` 或 substring。否则 token boundary、quote、
escape、平台 shell 语法都会进入 matcher，配置看似简短，实际很难审计。

prefix 只适合表达稳定的命令族，例如 `cargo test`。它不理解 flag semantics：允许
某个 subcommand 仍可能放行 config override、plugin/runner、output path、远端 URL 或
危险 flag。需要更具体的 deny、专用 normalizer，或暂时返回 `unmatched`。

### 2. executable 必须有可验证身份

仅匹配 `argv[0] == "git"` 不足以阻止 PATH spoofing。建议 rule 同时记录：

- 配置中的 logical name；
- 允许的 canonical executable path 集合，或受信 resolver/search roots；
- 实际 resolved path；必要时记录 file identity/version/hash 供审计，而不是把 hash
  强制作为 v1 稳定配置契约；
- resolved executable 是否位于 guest 可写目录。

若无法解析到可信 executable，允许 basename rule 只会制造“规则命中了，但执行的
不是用户以为的程序”的假象。

### 3. shell、解释器和脚本不能共享普通 prefix 语义

建议区分：

- **direct exec**：可直接对 executable + argv 求值；
- **script file**：同时验证 interpreter identity、canonical script path 及其参数；
- **inline code**：`python -c`、`node -e`、`sh -c` 等，默认不可持久化 allow；
- **shell command string**：只有 parser 能完整、无歧义地拆成 simple exec segment 时
  才逐段求值，取最严格结果；
- **opaque/parse failure**：interactive 时交给一次 approval，headless deny。

仅检查顶层 shell/解释器会漏掉其内部任意 child exec。若 Moonrun 无法拦截 descendant
`exec`，prefix rule 只能控制最初的 spawn，不能声称约束整个 process tree。

### 4. env、cwd 与 path 是独立 policy dimension

env assignment 不应混成 argv token。至少应识别：

- `PATH` 及 loader/search path；
- `LD_*`、`DYLD_*` 和平台等价注入变量；
- interpreter/module/plugin/config lookup 变量；
- secret-bearing inherited environment。

同样的 command 在不同 cwd 可能解析不同脚本、配置和 relative path。文件参数应经过
platform-aware canonicalization 和 symlink 检查，再交给 file policy；process prefix
不能替代 filesystem capability。

### 5. precedence 应单调收紧

安全核心宜采用所有命中取最严格，而不是 last-match-wins：

```text
managed/system deny > lower-layer deny > allow > unmatched
```

低层配置、plugin/hook、session approval 只能保持或收紧 managed result。若要支持
“宽 allow + 窄 deny”，strictest-wins 已能表达，不必依赖脆弱的规则排列顺序。

### 6. persistence 与规则求值分离

用户的 “allow once” 不应自动变成 durable prefix。若未来支持“记住”：

- UI 展示 canonical executable、准确 token prefix、scope 和正反例；
- 由 policy layer 生成候选 amendment，approval layer 只回答是否接受；
- shell/inline-code/destructive/含敏感 env 的请求标为 non-persistable；
- 保存 rule source、rule id、approver、scope、created/revoked time；
- 规则修改与一次 tool decision 分开审计。

## Moonrun 当前实现

当前实现有一个 deny-by-default 的 policy mode：

- 不传 `--policy` 时，`Policy::allow_all()` 用 `None` 表示保留历史上的不受限行为。
  传入 policy file 后，`fs`、`net`、`env` 和 `process` 即使省略也都会实例化为默认拒绝；
  `process` 支持粗粒度 `spawn: bool` 和 scoped `allow` entries。
- `PolicyConfig` 使用 `deny_unknown_fields`，这是继续演进 schema 的好基础。parser 实际
  同时支持 JSON 与 TOML：`.json` 或以 `{` 开头按 JSON 解析，其余按 TOML；但 README
  与 CLI help 已统一称为 JSON 或 TOML policy。
- `SpawnUnix` / `SpawnWindows` job 真正交给 worker 前调用平台对应的 policy matcher；因此
  检查点已经位于 job 的 cwd/options 等 setter 完成之后、OS spawn 之前，sync runner
  与 worker 共用同一检查路径。policy mode 还只允许等待当前 Moonrun 实例所启动并
  追踪的 child PID。
- `process.spawn = true` 是明确记录在文档里的粗粒度 escape hatch：child 获得 host
  user 的 ambient filesystem、network 与 process access，现有 `fs` / `net` policy
  不会约束 child 或 descendant。

### Unix 与 Windows 的表示不对称

Unix `SpawnUnix` job 已保留 `path: OsString`、完整 `args: Vec<OsString>`、env、cwd 和
stdio。这里 `path` 才是待执行程序；正常 MoonBit API 会把同一个 program 放进
`args[0]`，首版 matcher 要求这个 canonical invariant 成立，再从 `args[1]` 开始匹配。
Unix runner 对带 `/` 的 path 调 `posix_spawn`，对不带
`/` 的名称调 `posix_spawnp`；后者的实际目标受 PATH/cwd 解析影响。POSIX 对二者的
语义见 [`posix_spawn`](https://pubs.opengroup.org/onlinepubs/9799919799/functions/posix_spawn.html)。

Windows `SpawnWindows` job 则只保存一个 guest 提供的 `command_line: OsString`，最终
调用 `CreateProcessW(NULL, command_line, ...)`。微软
[`CreateProcessW` 文档](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw)
明确说明：`lpApplicationName == NULL` 时，系统从 command line 推断 executable。正常
MoonBit API 在进入 host import 前已经用 `write_arg_with_windows_escape` 编码 program
和每个 argument，避免未引用空格造成的经典歧义。首版 matcher 对 policy rule 应用
同一编码，并要求完整 prefix 后是 command-line 结束或单个 token 分隔空格；它不尝试
把任意 raw command line 反解析回 argv，也不声称识别 OS 最终选中的 executable。

### env 与 executable lookup

guest 可通过 Moonrun sys API 修改 policy-owned environment，spawn builder 也携带独立
env/cwd。即使 program 与 args prefix 不变，`PATH`、`LD_*`、`DYLD_*`、module/plugin
lookup 和 tool-specific config env 仍可能改变实际执行或行为。当前 Linux cwd 兼容
路径还会显式读取 host `PATH` 做 executable lookup。

首版不额外约束这些维度，只在文档中明确风险。若未来目标升级为抵御不可信 guest，
仅检查 prefix 不够，还必须冻结或约束 child env/cwd、绑定 OS 实际执行的 executable
identity，并增加 child process-tree sandbox。

## 对 Moonrun 配置的建议形状

### 推荐的 v1

首版采用下面的 schema。它贴合 Moonrun 现有 fs/net allowlist 风格，并避免先引入
通用 decision DSL：

```toml
[[process.allow]]
program = "/usr/bin/rustc"

[[process.allow]]
program = "/usr/bin/git"
args_prefix = ["status"]

[[process.allow]]
program = "/usr/bin/git"
args_prefix = ["diff", "--no-ext-diff"]
```

等价 JSON：

```json
{
  "process": {
    "allow": [
      { "program": "/usr/bin/rustc" },
      { "program": "/usr/bin/git", "args_prefix": ["status"] },
      { "program": "/usr/bin/git", "args_prefix": ["diff", "--no-ext-diff"] }
    ]
  }
}
```

建议语义：

- `program` 精确匹配调用方请求的逻辑 program 字符串，不要求或承诺 resolved path。
  Windows 会与 MoonBit spawn 一样补 `.exe`（已有 `.exe` / `.com` 时不补）并规范编码。
  因此 `PATH`、cwd、symlink 和环境仍可能改变 OS 最终执行的文件；更强身份保证需要
  单独的 resolved/absolute 模式或 child sandbox，而不是继续堆字符串规则。
- 可省略 `args_prefix`，表示允许该 program 的任意参数；显式空数组与省略等价。
  非空 `args_prefix` 匹配 argv0 之后的参数，按 `OsString` token 精确比较；
  `["status"]` 可匹配 `status --short`，但不匹配 `statusx`。不支持 shell parsing、
  substring、glob 或 regex。program-only rule 是刻意提供的宽规则，应在文档和审计中
  保持醒目，而不是借字符串空值表达。
- 多个 allow entry 是 OR。`process` 缺失、`allow` 缺失/为空或请求未命中时都 deny。
  配置字符串只能表达 Unicode；scoped rule 遇到不可表示的 native token 时 fail closed。
- 保留现有 `process.spawn = true` 作为醒目的 allow-all escape hatch，但 parser 应拒绝
  同时设置 `spawn = true` 与 `allow`，避免用户误以为 scoped entries 仍在生效。
- v1 不放 `effect`、deny entry、rule ordering 或 `ask`。如果之后确实出现 managed/user/
  project 多层 policy，再引入 deny-dominant 的合并语义，而不是 last-match-wins。
- shell、通用解释器、inline code 和会委托任意 runner/plugin 的工具默认不应进入持久
  allowlist。允许 `cargo test` 之类命令也仍会运行项目 build script，应按 workload 的
  信任模型判断，而不能由 prefix matcher 替用户推断安全性。

如果 rule 还要约束 cwd、env 或文件参数，应增加独立、类型化字段并分别求值，不要把
这些维度编码进 command string。每条 allow entry 最好支持 `match` / `not_match` 加载期
测试，但它们是配置自测样例，不参与 runtime 授权。

### approval 与 child sandbox 放在别处

Moonrun 当前是 headless runtime，没有 approval channel、session grant store 或 managed
policy layering。因此，不建议现在在 Moonrun policy file 中增加 `ask`、`approval` 或
尚未实现的 `child_sandbox` 字段：没有 channel 时未命中应稳定地 deny。

未来若 embedder/harness 提供交互层，静态 matcher 可以在内部返回 `unmatched` 并把它
交给一次性 approval；显式/managed deny 仍是 terminal。approval 只授权这一次 spawn，
不能隐式扩大 Moonrun fs/net/env 或 child 的 OS capabilities。child sandbox 则应是独立
enforcement backend，覆盖获准 child 及其 descendants，而不是 prefix rule 的副作用。

推荐 matcher/诊断输出不止一个 boolean：

```text
decision, matched_rule_ids, normalized_request, reason, policy_layers
```

并提供类似 `moonrun policy check -- ...` 的 dry-run：显示 resolved executable、argv
tokens、cwd/env 风险、所有命中规则、最终决定，以及为何无法解析。每条规则支持
load-time `match` / `not_match` 示例，避免 policy 改动后静默漂移。

## 证据限制

- Codex Rules 仍标为 experimental；DeepSeek Harness 是 developer preview；OpenCode
  V2 处于 `dev` 分支迁移期。这些不是稳定兼容标准。
- OpenCode 当前产品文档与 V2 dev source 在 Bash parser 和 reusable prefix approval
  上存在差异，本文已分别标注，未推断尚未落地的行为。
- Claude Code 没有公开实现源码，本节只能验证官方文档所声明的契约，不能审计其
  parser 或 matcher 实现。
- DeepSeek Harness 没有公开 prefix matcher、shell segment policy、规则 precedence
  或 persistent grant schema；不能把它当作 prefix-rule 参考实现。
- Deno 的 `--allow-run` 只按 program scope，不是 argv-prefix 系统；其价值是展示
  executable allowlist 的权限边界。
- 链接指向截至 2026-08-18 的官方文档与官方仓库活动分支。preview/dev 分支可能继续
  变化；落地设计前应 pin 具体版本/commit 再复核。
