# Finding toolchain binaries for `moon`

`moonutil::toolchain` owns executable resolution for MoonBit toolchain
programs and executable environment overrides.

MoonBit tool executables are selected in this order:

1. an executable specified by its override environment variable
2. the executable under the resolved toolchain root
3. an executable found through `PATH`

`moonx` and `moon_cove_report` have no executable override, so their lookup
starts at step 2.

Override values use the same platform executable lookup as other tool names.
They may be bare names, explicit relative paths, or absolute paths. Successful
resolution always produces an absolute path, including when `PATH` itself
contains relative entries. An override that cannot be resolved is left intact
so its command keeps the existing error-reporting behavior.

Whenever Moon delegates to a child before applying `-C` to its own process,
executable lookup and the child's working directory use the same effective
command directory. This applies to early `moon cram` and `moon help ide`
delegation. Relative `moon-cram` overrides and relative `PATH` entries are
resolved from that directory.

`moonlex.wasm` and `moonyacc.wasm` are payload files passed to `moonrun`, not
executables. Their overrides therefore use file-path semantics instead of
executable lookup.

The cached entry point is `moonutil::toolchain::BINARIES`. Build engines should
use it instead of duplicating tool lookup rules.

Binary resolution is demand-driven. The cache must not expose an aggregate
inventory API: code for display, diagnostics, or tests must never enumerate all
entries or otherwise force resolution of the complete executable inventory.
An unavailable unused executable must not make an operation fail.
