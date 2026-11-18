# Config JSON Schema

## moon.mod.json

The canonical JSON representation uses the same field names as the corresponding
module declarations. `import` maps module names to dependency specifications;
`rule` is an array even when it contains only one rule.

```json
{
  "name": "example/app",
  "import": { "example/dep": "1.2.3" },
  "warnings": "+a",
  "preferred_target": "js",
  "supported_targets": "js",
  "rule": [
    { "name": "generate", "command": "generator $input -o $output" }
  ]
}
```

The reader also accepts the legacy names `deps`, `warn-list`, `preferred-target`,
and `supported-targets`, and the legacy single-object `rule` form. Serialization
uses the canonical names and rule array. A canonical name and its legacy alias
cannot both be set, and rule names must be unique.

Project builds use `moon.mod`. `moon fmt` migrates either JSON spelling to that
format; `--check` and `--warn` preserve the input files.

<iframe src="source/mod_json_schema.html" width="100%" height="500px" frameborder="0"></iframe>

## moon.pkg.json
<iframe src="source/pkg_json_schema.html" width="100%" height="500px" frameborder="0"></iframe>