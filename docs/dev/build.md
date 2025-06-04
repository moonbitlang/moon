# How a MoonBit package is compiled

## High-level constructs

A **module** in MoonBit is the unit of dependency version resolution.
The root of a module is signified by a file named `moon.mod.json`.
A module recursively contain all directories and files under its root,
until another module is met.

A module may contain one or more **package**s,
which is the unit of compilation in MoonBit.
A package contains all files (not directories) within its containing directory,
and signified by a file named `moon.pkg.json`.
All the code within a single package is compiled at once using the `moonc` compiler,
while different packages are compiled in different calls to the compiler.

A module may depend on other modules,
which in turn fetches the packages contained by the module.
A package may depend on other packages within its containing module,
or within the modules its containing module depends on.
Cyclic dependencies are currently prohibited both in module and package level.

A package can be **virtual**,
which is similar to virtual modules in OCaml.
A virtual package by default has only the public members as an interface,
but no implementation.
The package can be **implemented** by another package or itself.

## Anatomy of a package

There are a lot of knobs to tweak within a package,
but we will focus on those related to how it is compiled in this section.
This section assumes a non-virtual package.

There are 4 kinds of source files within each package:

- **Source**. These are the regular `.mbt` files within a package.
  All files in the current package with `.mbt` extension
  and does not to any below belong to this kind.
- **Whitebox test**. These files are suffixed `_wbtest.mbt`.
- **Blackbox test**. These files are suffixed `_test.mbt`.
- **C stub**. These are C files manually specified in each package,
  and recognized by the build system to be built.

There are 4 major build targets within each package,
each with its own list of source files:

- **Source**: Source and C stub.
- **Inline test**: Source (with test flags enabled) and C stub.
- **Whitebox test**: Whitebox test source.
- **Blackbox test**: Blackbox test source.
