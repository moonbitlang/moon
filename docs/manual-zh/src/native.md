# 原生构建配置

## 内存分配器

`MOONBIT_ALLOCATOR` 用于选择原生运行时构建所使用的内存分配器：

- `mimalloc`：按 mimalloc 配置编译运行时，并链接工具链提供的
  `libmoonbitrun.o` 支持目标文件。
- `system`：按系统分配器配置编译运行时，不链接 `libmoonbitrun.o`。

未设置该变量时，Moon 保留当前平台和工具链的默认行为。如果所选平台或
工具链不提供相应的支持目标文件（包括 Windows 和 TCC），选择 `mimalloc`
将在构建规划阶段报错。
