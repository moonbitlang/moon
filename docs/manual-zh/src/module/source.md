# 源码目录

`source` 字段用于指定模块的源码目录。

它必须为当前 `moon.mod.json` 文件所在目录的子目录。且必须为相对路径

当使用 `moon new` 命令创建模块时，会自动生成一个 `src` 目录，并且 `source` 字段的默认值为 `src`。

```json
{
  "source": "src"
}
```
