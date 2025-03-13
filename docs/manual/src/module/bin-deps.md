# Binary Dependencies

- The configuration for "bin-deps" is as follows:

```json
"bin-deps": {
    // simple format, install all main packages
    "hackwaly/moonyacc": "0.2.0",
    // detailed format, local deps? specify which main pkg to install?
    "username/flash": {
        // local deps
        "path": "/Users/flash/projects/my-project",
        // package names to install
        "bin-pkg": [
            "main-js",
            "main-wasm"
        ]
    }
}
```

- Running `moon add hackwaly/moonyacc --bin` will automatically add hackwaly/moonyacc as a binary dependency in moon.mod.json. When running moon build | check | test, it will automatically compile and install the binary artifacts of hackwaly/moonyacc to `.mooncakes/hackwaly/moonyacc/`

- In ["pre-build"](../package/pre-build.md), you can access it using `$mod_dir/.mooncakes/hackwaly/moonyacc/moonyacc`