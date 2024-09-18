# js 后端链接选项

#### 可配置选项

- `exports` 选项用于指定 JavaScript 模块的导出函数名。

  例如，如下配置将当前包中的 `hello` 函数导出为 JavaScript 模块的 `hello` 函数。在 JavaScript 宿主中，可以通过 `hello` 函数来调用当前包中的 `hello` 函数。

  ```json
  {
    "link": {
      "js": {
        "exports": [
          "hello"
        ]
      }
    }
  }
  ```

- `format` 选项用于指定 JavaScript 模块的输出格式。

  目前支持的格式有：
  - `esm`
  - `cjs`
  - `iife`

  例如，如下配置将当前包的输出格式指定为 ES Module。

  ```json
  {
    "link": {
      "js": {
        "format": "esm"
      }
    }
  }
  ```

  
