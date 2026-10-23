# Config JSON Schema

## moon.mod.json

The legacy `scripts.postadd` hook is accepted in both `moon.mod` and
`moon.mod.json`, but is never executed. Moon warns when synchronizing
dependencies, fetching a module, or installing a binary, including when reusing
downloaded modules or the shared dependency source cache. `--quiet` suppresses
the warning.

Remove obsolete `postadd` declarations. If your workflow needs toolchain setup
hooks, please [open an issue](https://github.com/moonbitlang/moon/issues) describing
your use case so we can discuss it. `MOON_IGNORE_POSTADD` is no longer needed and
has no effect.

<iframe src="source/mod_json_schema.html" width="100%" height="500px" frameborder="0"></iframe>

## moon.pkg.json
<iframe src="source/pkg_json_schema.html" width="100%" height="500px" frameborder="0"></iframe>
