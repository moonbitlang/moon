# Compiler Commands Reference

See doc comments in: [`moonbuild_rupes_recta::build_lower::compiler`](../../../crates/moonbuild-rupes-recta/src/build_lower/compiler/mod.rs).

## Wasm allocator

The command adapter captures `MOON_WASM_NEW_ALLOCATOR=1` in `BackendConfig::Wasm`.
See [Wasm build configuration](../../manual/src/wasm.md) for usage. Lowering
passes `-allocator tlsf-mbt` to every `moonc link-core` action for that backend,
including library, executable, test, and benchmark links, for both Wasm binary
and WAT output, and to `moonc wasm-ld` under
[separate compilation](#wasm-separate-compilation). Other compiler phases and target backends are unaffected.
When the variable is unset or has a value other than `1`, Moon omits the flag.

## Reference cycle collection

`MOON_COLLECT_REF_CYCLE=1` adds `-enable-trial-deletion` to `moonc link-core`
for the linear-memory Wasm backend (`--target wasm`), including WAT output,
and to `moonc wasm-ld` under [separate compilation](#wasm-separate-compilation).
The setting applies to project and standalone executable builds, runs, tests,
and benchmarks. It does not add flags to package compilation or checking.
WasmGC and JavaScript keep their existing garbage collection behavior.
Native C compilation uses the corresponding runtime macro described in
[Native C toolchain resolution](native-c-toolchain-resolution.md).

The variable is disabled when unset or set to any value other than `1`.
The selected flags are part of execution commands, so toggling the setting
invalidates affected incremental builds and cached actions.

## Wasm separate compilation

The command adapter captures `MOON_SEPARATE_COMPILATION=1` in
`BackendConfig::Wasm`; any other value or an unset variable disables it. For
the executables that planning links from objects (see
[the package build process](build.md#wasm-separate-compilation-experimental)):

- the main object is a `link-core` of the executable's target with a `.o`
  output and without `-wasm-module-name`;
- each library object is a `link-core` of its package with
  `-pkg-type library`, a `.o` output, and no `-pkg-config-path`, exports or
  module link flags;
- `moonc wasm-ld <objects> -o <executable>` links them, with `-g` when
  `link-core` gets it, and the `-wasi`, `-allocator tlsf-mbt` and
  `-enable-trial-deletion` settings of the objects.
