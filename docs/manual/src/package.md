# Package configuration

`moon.pkg` supports direct declarations for the following package settings.
Each example is an independent setting; virtual-package declarations,
implementations, and consumers normally belong to different packages.

| Declaration | Meaning |
| --- | --- |
| `proof_enabled = true` | Enable proof workflows for the package. Defaults to `false`. |
| `bin_name = "app"` | Set the installed binary name. |
| `bin_target = "native"` | Set the backend when built as a binary dependency: `wasm`, `wasm-gc`, `js`, `native`, or `llvm`. |
| `max_concurrent_tests = 4` | Set test concurrency using an unsigned 32-bit integer. |
| `regex_backend = "table"` | Select `auto`, `block`, `table`, or `runtime` for regex compilation. |
| `implement = "user/module/interface"` | Implement a virtual package. |
| `overrides = ["user/module/implementation"]` | Select virtual-package implementations for this consumer. |
| `virtual(has_default: true)` | Declare a virtual package with a default implementation; use `false` when it has no default. |

These declarations preserve the behavior of the corresponding legacy fields in
`options(...)`. Legacy spellings remain accepted there, including `proof-enabled`,
`bin-name`, `bin-target`, `max-concurrent-tests`, `regex-backend`, and the underscore
aliases. `virtual_pkg` is also accepted as a legacy alias of `virtual` inside
`options(...)`.

Declare each setting only once. Using both a direct declaration and its legacy
option is an error, even if the values agree or the option uses another spelling.
For example, do not combine `proof_enabled = true` with
`options("proof-enabled": true)`.

Structured `link`, `native-stub`, and per-file `targets` settings still belong in
`options(...)`.

These declarations require matching compiler support. Compilers that do not
recognize them report `Invalid configuration` (`E4192`); use the legacy
`options(...)` form with those toolchains. Formatting and JSON-to-DSL migration
are provided by the toolchain's `moonfmt`. Older formatter versions may rewrite
direct declarations into their equivalent `options(...)` form.
