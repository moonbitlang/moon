# Config JSON Schema

## moon.mod.json

`moon.mod` 和 `moon.mod.json` 仍接受旧的 `scripts.postadd` 配置，但不再执行它。
同步依赖、下载模块或安装二进制程序时，Moon 会发出警告；复用已下载的模块或共享依赖源码缓存时也一样。
`--quiet` 会隐藏该警告。

请移除不再使用的 `postadd` 配置。如果你的工作流需要工具链设置钩子，欢迎
[提交 issue](https://github.com/moonbitlang/moon/issues)，说明具体使用场景以便讨论。
`MOON_IGNORE_POSTADD` 已不再需要，也不再产生任何效果。

<iframe src="source/mod_json_schema.html" width="100%" height="500px" frameborder="0"></iframe>

## moon.pkg.json
<iframe src="source/pkg_json_schema.html" width="100%" height="500px" frameborder="0"></iframe>
