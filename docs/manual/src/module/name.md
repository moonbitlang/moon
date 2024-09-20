# Module Name

The `name` field is used to specify the name of the module, and it is required.

```json
{
  "name": "example",
  ...
}
```

The module name can contain letters, numbers, `_`, `-`, and `/`.

For modules published to [mooncakes.io](https://mooncakes.io), the module name must begin with the username. For example:

```json
{
  "name": "moonbitlang/core",
  ...
}
```