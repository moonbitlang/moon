# Package configuration

`moon.pkg` supports direct declarations for the following package settings.
Each example is an independent setting.

| Declaration | Meaning |
| --- | --- |
| `experimental = true` | Mark the package as experimental and warn its direct importers. Defaults to `false`. |
| `proof_enabled = true` | Enable proof workflows for the package. Defaults to `false`. |
| `max_concurrent_tests = 4` | Set test concurrency using an unsigned 32-bit integer. |
| `regex_backend = "table"` | Select `auto`, `block`, `table`, or `runtime` for regex compilation. |
| `virtual(has_default: true)` | Declare a virtual package with a default implementation; use `false` when it has no default. |
| `virtual(implement: "user/module/interface")` | Implement the given virtual package. |

These declarations preserve the behavior of the corresponding legacy fields in
`options(...)`. Legacy spellings remain accepted there, including `proof-enabled`,
`max-concurrent-tests`, `regex-backend`, and the underscore aliases. Inside
`options(...)`, `virtual_pkg` is also accepted as a legacy alias of `virtual`, and
`implement` corresponds to `virtual(implement: ...)`.

A package declares `virtual(...)` at most once: it either is a virtual package or
implements one, so `has_default` and `implement` cannot be combined.

Declare each setting only once, either directly or in `options(...)`. For
example, do not combine `proof_enabled = true` with
`options("proof-enabled": true)`.

Other settings, including `bin-name`, `bin-target`, `overrides`, structured
`link`, `native-stub`, and per-file `targets`, still belong in `options(...)`.

These declarations require matching compiler support. Compilers that do not
recognize them report `Invalid configuration` (`E4192`); use the legacy
`options(...)` form with those toolchains. Formatting and JSON-to-DSL migration
are provided by the toolchain's `moonfmt`. Older formatter versions may rewrite
direct declarations into their equivalent `options(...)` form.

## Standalone scripts

An explicit `.mbtx` path selects a standalone script for `moon build`,
`moon check`, `moon test`, and `moon run`, including inside a project. Scripts
declare their package imports in front matter using `moon.pkg` syntax:

```moonbit
----
import {
  "moonbitlang/core/env",
}
----

fn main {
  println(@env.args())
}
```

Only declared packages are imported. With no import declaration, an empty
`import {}`, or only a `permission(...)` declaration, the package import list is
empty. Language builtins and the ordinary prelude remain available. References
such as `@env` require explicit imports; suppressing warnings does not allow
undeclared core imports. Versioned paths, aliases, and `*` imports remain
supported. This also applies to `moonx`, `moon run -`, `moon run -e`, and `.mbtx`
prebuild scripts.

For `moon check`, `moon run`, and `moon test`, ordinary `.mbt` and `.mbt.md`
paths select their containing package and require project context. Running a selector compiles the whole
package and requires a main package; testing selects tests in the requested file.
Outside a project, these paths report the ordinary project-not-found error.
Inside a project, files without a containing package report the ordinary
package-selection error.

## Standalone Markdown

`moon check file.mbt.md` and `moon test file.mbt.md` treat a Markdown file as
standalone when its YAML front matter declares `moonbit.import`, including an
empty list (`[]`) or map (`{}`). This works inside and outside projects and uses
only that file and its declared dependencies. No `fn main` is required.

````markdown
---
moonbit:
  import:
    - path: moonbitlang/core/int
      alias: integer
  backend: js
---
```mbt check
test { assert_eq(@integer.MAX_VALUE, 2147483647) }
```
````

Imports accept strings, objects with `path` and an optional `alias`, or a map
from paths to aliases. An external module requires a version in at least one
import: `example/lib@1.2.3/first` and `example/lib/second` share that version.
`example/lib@1.2.3` imports the module's root package. Conflicting versions are
errors, and `moonbitlang/core` imports must not specify versions. Markdown
retains the compiler's usual standard-library import compatibility behavior;
the stricter `.mbtx` import policy does not apply.

Backend selection uses `--target` first, then `moonbit.backend`, then the default
backend. Multiple CLI backends and `--target all` are supported. Explicit
`moon fmt file.mbt.md` and `moon check file.mbt.md --fmt` also work outside a
project. Formatting checks YAML syntax and requires the header and `moonbit`
section to be mappings. The presence of `moonbit.import` selects standalone
formatting even when its value is null or unfinished. Formatting does not
validate import values, paths, versions, backends, or `deps`/`import` conflicts.
`moon check` and `moon test` validate the full configuration;
`moon check --fmt` does so after its formatting stage. Build outputs and private
dependencies are isolated under `_build/file.mbt.md` by default.

Without `moonbit.import`, Markdown continues to select its containing package.
`moonbit.deps` alone does not enable standalone mode or automatically import
packages. During build, check, run, and test, combining `deps` and `import` is an
error, and invalid YAML, imports, or backends report the file path without falling
back to project mode. Standalone Markdown passed to `moon build` or `moon run`
reports an error directing users to `moon check` or `moon test`.

## Experimental packages

Declare `experimental = true` at the top level of a package's `moon.pkg` to
warn packages that directly depend on it. This includes ordinary, whitebox-test,
and blackbox-test imports. Only imports enabled for a requested backend are
checked. Each importing package receives one warning per experimental dependency,
even when several test targets or backends use it. The warning does not make the
command fail, and `--quiet` suppresses it.

MoonBuild owns this check. Because the compiler also reads `moon.pkg`, compiling
with this declaration requires a compiler whose manifest parser accepts
`experimental`; older parsers may report `Invalid configuration` (`E4192`).

In the importing package's `moon.pkg`, acknowledge specific dependencies by their
full package names:

```moonbit
import { "user/library/unstable" }
options("allow-experimental-packages": ["user/library/unstable"])
```

Set `options("allow-experimental-packages": true)` to acknowledge all experimental
dependencies. `false`, an empty list, or omitting the option acknowledges none.
List entries match full package names exactly, rather than aliases or prefixes.
The option applies only to that package's direct dependencies; it does not
propagate to other packages. A stable wrapper around an experimental package does
not itself become experimental.

Legacy `moon.pkg.json` accepts `"experimental": true` and
`"allow-experimental-packages": true` (or a string list) as top-level fields.

## Lexer and parser generation

Moon no longer automatically runs `moonlex` or `moonyacc` for `.mbl` or `.mby`
files. Declare an explicit custom rule that produces `.mbt` files instead. For
example, if `my-lexer` is available as a command:

```moonbit
rule(name: "lexer", command: "my-lexer $input -o $output")
dev_build(rule: "lexer", input: "lexer.mbl", output: "lexer.mbt")
```

A `.mbl` or `.mby` file directly in a package directory must appear in a custom
rule's declared inputs or outputs (legacy `pre-build` entries also work).
Otherwise Moon reports a migration error. Include generated `.mbt` files when
publishing: dependency consumers do not run package prebuild rules.
