# 链接选项

moon 默认只会链接 `is-main` 为 `true` 的包，如果需要链接其他包，可以通过 `link` 选项指定。

`link` 选项用于指定链接选项，它的值可以为布尔值或一个对象。

- `link` 值为 `true` 时，表示需要链接该包。构建时所指定的后端不同，产物也不同。

  ```json
  {
    "link": true
  }
  ```

- `link` 值为对象时，表示需要链接该包，并且可以指定链接选项，详细配置请查看对应后端的子页面。