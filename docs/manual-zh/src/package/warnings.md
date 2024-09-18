# warn 列表

关闭对应的编译器预设警告编号。

例如，如下配置中 `-2` 代表关闭编号为 2(Unused variable) 的警告

```json
{
  "warn_list": "-2",
}
```

可用 `moonc build-package -warn-help` 查看编译器预设的警告编号

```
$ moonc -v                      
v0.1.20240914+b541585d3

$ moonc build-package -warn-help
Available warnings: 
  1 Unused function.
  2 Unused variable.
  3 Unused type declaration.
  4 Redundant case in a pattern matching (unused match case).
  5 Unused function argument.
  6 Unused constructor.
  7 Unused module declaration.
  8 Unused struct field.
 10 Unused generic type variable.
 11 Partial pattern matching.
 12 Unreachable code.
 13 Unresolved type variable.
 14 Lowercase type name.
 15 Unused mutability.
 16 Parser inconsistency.
 18 Useless loop expression.
 19 Top-level declaration is not left aligned.
 20 Invalid pragma
 21 Some arguments of constructor are omitted in pattern.
 22 Ambiguous block.
 23 Useless try expression.
 24 Useless error type.
 26 Useless catch all.
  A all warnings
```
