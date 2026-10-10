# Package configuration

`moon.pkg` supports direct declarations for the following package settings.
Each example is an independent setting.

| Declaration | Meaning |
| --- | --- |
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
