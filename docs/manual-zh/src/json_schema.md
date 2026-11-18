# Config JSON Schema

## moon.mod.json

支持的用户配置格式是旧 JSON 和 `moon.mod` 自定义格式。旧 JSON 保持已有字段名：

```json
{
  "name": "example/app",
  "deps": { "example/dep": "1.2.3" },
  "warn-list": "+a",
  "preferred-target": "js",
  "supported-targets": "js"
}
```

修改 JSON 配置的命令仍按此格式写回。`moon fmt` 将它迁移为 `moon.mod`；
`--check` 和 `--warn` 保留输入文件。

### 内部标准化表示

以下 schema 描述 Moon 的内部标准化 JSON 表示，供维护使用。它不用于校验用户编写的
`moon.mod.json`，也不代表受支持的输入格式或稳定的 JSON 导出接口。

<iframe src="source/mod_json_schema.html" width="100%" height="500px" frameborder="0"></iframe>

## moon.pkg.json
<iframe src="source/pkg_json_schema.html" width="100%" height="500px" frameborder="0"></iframe>
