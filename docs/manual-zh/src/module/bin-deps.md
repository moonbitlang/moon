# 二进制依赖

- "bin-deps" 的配置如下：

```json
"bin-deps": {
    // simple foramt, install all main package
    "hackwaly/moonyacc": "0.2.0",
    // detailed foramt, local deps? specificy which main pkg to install?
    "username/flash": {
        // local deps
        "path": "/Users/flash/projects/my-project",
        // package name to install
        "bin-pkg": [
            "main-js",
            "main-wasm"
        ]
    }
}
```

- 运行 `moon add hackwaly/moonyacc --bin` 可自动在 moon.mod.json 中将 hackwaly/moonyacc 添加为二进制依赖，运行 moon build | check | test，会自动编译并安装 hackwaly/moonyacc 的二进制产物到 `.mooncakes/hackwaly/moonyacc/` 中

- 在 ["pre-build"](../package/pre-build.md) 中，可以用 `$mod_dir/.mooncakes/hackwaly/moonyacc/moonyacc` 访问
