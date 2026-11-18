# Config JSON Schema

## moon.mod.json

Supported user configuration formats are the legacy JSON format and the
`moon.mod` DSL. Legacy JSON keeps its existing field names:

```json
{
  "name": "example/app",
  "deps": { "example/dep": "1.2.3" },
  "warn-list": "+a",
  "preferred-target": "js",
  "supported-targets": "js"
}
```

Commands that update a JSON manifest retain this format. `moon fmt` migrates it
to `moon.mod`; `--check` and `--warn` preserve the input files.

### Internal normalized representation

The schema below describes Moon's internal normalized JSON representation for
maintenance. It is not a schema for user-authored `moon.mod.json` files and does
not establish a supported input format or a stable JSON export API.

<iframe src="source/mod_json_schema.html" width="100%" height="500px" frameborder="0"></iframe>

## moon.pkg.json
<iframe src="source/pkg_json_schema.html" width="100%" height="500px" frameborder="0"></iframe>
