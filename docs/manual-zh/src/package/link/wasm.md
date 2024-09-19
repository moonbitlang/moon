# wasm 后端链接选项

#### 可配置选项

- `exports` 选项用于指定 wasm 后端导出的函数名。

  例如，如下配置将当前包中的 `hello` 函数导出为 wasm 模块的 `hello` 函数, `foo` 函数导出为 wasm 模块的 `foo` 函数。在 wasm 宿主中，可以通过 `hello` 和 `bar` 函数来调用当前包中的 `hello` 和 `foo` 函数。

  ```json
  {
    "link": {
      "wasm": {
        "exports": [
          "hello",
          "foo:bar"
        ]
      },
    }
  }
  ```

- `heap-start-address` 选项用于指定 moonc 编译到 wasm 后端时能够使用的线性内存的起始地址。

  例如，如下配置将线性内存的起始地址设置为 1024。

  ```json
  {
    "link": {
        "wasm": {
          "heap-start-address": 1024
      },
    }
  }
  ```

- `import-memory` 选项用于指定 wasm 模块导入的线性内存。

  例如，如下配置将 wasm 模块导入的线性内存指定为 `env` 模块的 `memory` 变量。

  ```json
  {
    "link": {
      "wasm": {
        "import-memory": {
          "module": "env",
          "name": "memory"
        }
      },
    }
  }
  ```

- `export-memory-name` 选项用于指定 wasm 模块导出的线性内存名称。

  ```json
  {
    "link": {
      "wasm": {
        "export-memory-name": "memory"
      },
    }
  }
  ```
