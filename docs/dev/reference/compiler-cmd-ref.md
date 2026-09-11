# Compiler Commands Reference

See doc comments in: [`moonbuild_rupes_recta::build_lower::compiler`](../../../crates/moonbuild-rupes-recta/src/build_lower/compiler/mod.rs).

## Wasm allocator

The command adapter captures `MOON_WASM_NEW_ALLOCATOR=1` in `BackendConfig::Wasm`.
See [Wasm build configuration](../../manual/src/wasm.md) for usage. Lowering
passes `-allocator tlsf-mbt` to every `moonc link-core` action for that backend,
including library, executable, test, and benchmark links, for both Wasm binary
and WAT output. Other compiler phases and target backends are unaffected.
When the variable is unset or has a value other than `1`, Moon omits the flag.

## Reference cycle collection

`MOON_COLLECT_REF_CYCLE=1` adds `-enable-trial-deletion` to `moonc link-core`
for the linear-memory Wasm backend (`--target wasm`), including WAT output.
The setting applies to project and standalone executable builds, runs, tests,
and benchmarks. It does not add flags to package compilation or checking.
WasmGC and JavaScript keep their existing garbage collection behavior.
Native C compilation uses the corresponding runtime macro described in
[Native C toolchain resolution](native-c-toolchain-resolution.md).

The variable is disabled when unset or set to any value other than `1`.
The selected flags are part of execution commands, so toggling the setting
invalidates affected incremental builds and cached actions.
