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
