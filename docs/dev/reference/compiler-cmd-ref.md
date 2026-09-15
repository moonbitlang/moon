# Compiler Commands Reference

See doc comments in: [`moonbuild_rupes_recta::build_lower::compiler`](../../../crates/moonbuild-rupes-recta/src/build_lower/compiler/mod.rs).

## Wasm allocator

The command adapter captures `MOON_WASM_NEW_ALLOCATOR=1` in `BackendConfig::Wasm`.
See [Wasm build configuration](../../manual/src/wasm.md) for usage. Lowering
passes `-allocator tlsf-mbt` to every `moonc link-core` action for that backend,
including library, executable, test, and benchmark links, for both Wasm binary
and WAT output. Other compiler phases and target backends are unaffected.
When the variable is unset or has a value other than `1`, Moon omits the flag.
