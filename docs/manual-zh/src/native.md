# 原生构建配置

## 内存分配器

`MOONBIT_ALLOCATOR` 用于选择原生运行时构建所使用的内存分配器：

- `mimalloc`：按 mimalloc 配置编译运行时，并链接工具链提供的
  `libmoonbitrun.o` 支持目标文件。
- `system`：按系统分配器配置编译运行时，不链接 `libmoonbitrun.o`。

编译工具链自带的运行时源码和 `moonc link-core` 生成的 C 代码时都会定义该宏，
以保证程序内联的分配器与其链接的运行时一致。

未设置该变量时，Moon 保留当前平台和工具链的默认行为。如果所选平台或
工具链不提供相应的支持目标文件（包括 Windows 和 TCC），选择 `mimalloc`
将在构建规划阶段报错。

## 引用环回收

设置 `MOON_COLLECT_REF_CYCLE=1` 可启用实验性的引用环回收：

```sh
MOON_COLLECT_REF_CYCLE=1 moon run main --target native
MOON_COLLECT_REF_CYCLE=1 moon test --target wasm
```

该功能默认关闭。取消设置该变量或将其设为 `0` 即可关闭；只有值 `1` 会启用。

该设置适用于 `build`、`run`、`test` 和 `bench`，包括单文件模式以及 debug 和
release 构建。启用后，原生构建使用生成 C 代码的后端，即使同时设置了
`MOONBIT_NEW_NATIVE=1` 也是如此。两种原生内存分配器均受支持。
Wasm 构建需要支持 `-enable-trial-deletion` 的编译器。
该设置不影响 WasmGC、JavaScript 和 LLVM。
