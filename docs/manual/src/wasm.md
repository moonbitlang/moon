# Wasm build configuration

Set `MOON_WASM_NEW_ALLOCATOR=1` to enable the MoonBit TLSF allocator:

```sh
MOON_WASM_NEW_ALLOCATOR=1 moon build --target wasm
```

This applies to the `wasm` backend in `moon build`, `moon run`, `moon test`, and
`moon bench`, including `--output-wat`. It passes `-allocator tlsf-mbt` to
`moonc link-core` and requires a compiler that supports that option.

When unset or set to any value other than `1`, the compiler's default allocator
is used. Other target backends are unaffected.
