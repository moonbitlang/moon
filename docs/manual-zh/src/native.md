# 原生构建配置

## 后端选择

对于 `--target native`，macOS AArch64、Linux x86_64 和 Windows x86_64 的 debug
构建默认使用新 native 后端，由 `moonc` 直接生成目标文件。设置
`MOONBIT_NEW_NATIVE=0` 可切换到生成 C 代码的后端；其他值保持默认行为。
其他平台以及 release 构建使用 C 后端，即使设置 `MOONBIT_NEW_NATIVE=1` 也不例外。
如果选中的可执行包配置了 `link.native.cc-flags`，或 `MOON_CC` 指定了面向其他
目标的编译器，本次构建也会使用 C 后端。

Windows 的新 native 后端要求兼容 `cl` 的 MSVC 工具链，例如 `cl.exe` 或
`clang-cl.exe`，用于编译运行时、C stub 和链接可执行文件。

## 浮点运算

使用 GCC/Clang 的 Native C 编译默认通过 `-ffp-contract=off` 禁止浮点运算收缩，
保留乘法、加法等各步运算独立舍入的语义。MSVC 兼容编译器保持默认行为。

## 内存分配器

`MOONBIT_ALLOCATOR` 用于选择原生运行时构建所使用的内存分配器：

- `mimalloc`：按 mimalloc 配置编译运行时，并链接工具链提供的
  `libmoonbitrun.o` 支持目标文件。
- `system`：按系统分配器配置编译运行时，不链接 `libmoonbitrun.o`。

编译工具链自带的运行时源码和 `moonc link-core` 生成的 C 代码时都会定义该宏，
以保证程序内联的分配器与其链接的运行时一致。

未设置该变量时，Moon 保留当前平台和工具链的默认行为。如果所选平台或
工具链不提供相应的支持目标文件（包括 Windows、TCC 以及交叉编译），选择
`mimalloc` 将在构建规划阶段报错。

## 交叉编译

将 `MOON_CC`（以及按需设置的 `MOON_AR`）设为面向其他目标的编译器，例如执行
`zig cc -target x86_64-linux-musl "$@"` 的包装脚本。Moon 会以 `-dumpmachine`
运行该编译器以确定其目标。当目标的架构、操作系统或 ABI（例如 glibc Linux
主机上的 `musl`）与主机不同时，Moon 不会链接
MoonBit lib 目录中的预编译目标文件（`libmoonbitrun.o`、SIMDUTF 目标文件以及
`libbacktrace.a`），因为它们是为主机构建的。此时运行时使用系统分配器和标量
UTF 代码路径，且 panic 时不打印调用栈。

交叉编译仅为尽力支持，且只能通过全局的 `MOON_CC` 进行。包级别的 `cc` 不会改变
共享运行时或其他包的 C stub 的目标，因此不能用于交叉编译。`platform` cfg
以及预编译的 `core` 库仍然跟随主机。

## 引用环回收

设置 `MOON_COLLECT_REF_CYCLE=1` 可启用实验性的引用环回收：

```sh
MOON_COLLECT_REF_CYCLE=1 moon run main --target native
MOON_COLLECT_REF_CYCLE=1 moon test --target wasm
```

该功能默认关闭。取消设置该变量或将其设为 `0` 即可关闭；只有值 `1` 会启用。

该设置适用于 `build`、`run`、`test` 和 `bench`，包括单文件模式以及 debug 和
release 构建。原生构建的生成 C 代码后端和机器码后端（直接输出目标文件）均支持
引用环回收，也都支持两种原生内存分配器。启用回收不会改变原有的后端选择规则，
包括 `MOONBIT_NEW_NATIVE` 的设置。机器码构建需要支持 trial deletion 的编译器。
Wasm 构建需要支持 `-enable-trial-deletion` 的编译器。
该设置不影响 WasmGC、JavaScript 和 LLVM。
