# `moon bundle`

`moon bundle` consolidates all packages in a whole module into a single file. It is currently only used in `moonbitlang/core`, the standard library, to provide the many packages of the whole standard library within only one argument of the build commands.

The behavior of `moon bundle` is as following:

1. Build all packages like `moon build`
2. Use `moonc bundle`, and include all packages except virtual packages (`moonbitlang/core/abort`).
