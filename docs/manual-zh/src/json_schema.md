# Config JSON Schema

## moon.mod.json

规范 JSON 使用与模块声明对应的字段名。`import` 是模块名到依赖配置的映射；
`rule` 使用数组，即使只有一条规则也是如此。

```json
{
  "name": "example/app",
  "import": { "example/dep": "1.2.3" },
  "warnings": "+a",
  "preferred_target": "js",
  "supported_targets": "js",
  "rule": [
    { "name": "generate", "command": "generator $input -o $output" }
  ]
}
```

读取时仍接受旧名称 `deps`、`warn-list`、`preferred-target`、`supported-targets`，
以及单个对象形式的 `rule`。序列化统一使用规范名称和规则数组。
规范名称与对应的旧别名不能同时设置，规则名必须唯一。

项目构建使用 `moon.mod`。`moon fmt` 可以将两种 JSON 写法迁移到该格式；
`--check` 和 `--warn` 保留输入文件。

<iframe src="source/mod_json_schema.html" width="100%" height="500px" frameborder="0"></iframe>

## moon.pkg.json
<iframe src="source/pkg_json_schema.html" width="100%" height="500px" frameborder="0"></iframe>