# Wasm 构建配置

设置 `MOON_WASM_NEW_ALLOCATOR=1` 可启用 MoonBit 实现的 TLSF 内存分配器：

```sh
MOON_WASM_NEW_ALLOCATOR=1 moon build --target wasm
```

此设置适用于 `moon build`、`moon run`、`moon test` 和 `moon bench` 的 `wasm`
后端，包括 `--output-wat` 输出。Moon 会向 `moonc link-core` 传递
`-allocator tlsf-mbt`，因此需要编译器支持该选项。

未设置此变量或值不为 `1` 时，使用编译器的默认分配器。其他后端不受影响。
