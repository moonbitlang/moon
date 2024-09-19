# 预构建命令

字段 `"pre-build"` 用于指定预构建命令，预构建命令会在 `moon check|build|test` 等构建命令之前执行。

`"pre-build"`是一个数组，数组中的每个元素是一个对象，对象中包含 `input`，`output` 和 `command` 三个字段，`input` 和 `output` 可以是字符串或者字符串数组，`command` 是字符串，`command` 中可以使用任意命令行命令，以及 `$input`，`$output` 变量，分别代表输入文件、输出文件，如果是数组默认使用空格分割。

目前内置了一个特殊命令 `:embed`，用于将文件转换为 MoonBit 源码，`--text` 参数用于嵌入文本文件，`--binary` 用于嵌入二进制文件，`--text` 为默认值，可省略不写。`--name` 用于指定生成的变量名，默认值为 `resource`。命令的执行目录为当前 `moon.pkg.json` 所在目录。

```json
{
  "pre-build": [
    {
      "input": "a.txt",
      "output": "a.mbt",
      "command": ":embed -i $input -o $output"
    }
  ]
}
```

如果当前包目录下的 `a.txt` 的内容为
```
hello,
world
```

执行  `moon build` 后，在此 `moon.pkg.json` 所在目录下生成如下 `a.mbt` 文件

```
let resource : String =
  #|hello,
  #|world
  #|
```
