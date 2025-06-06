# Module and pacakges

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
which in turn makes all packages contained by the depended module available for import.
A package may import other packages within its containing module,
or within the modules its containing module depends on.
Cyclic dependencies are currently prohibited both in module and package level.

A package can be **internal** to restrict importing,
see the [Internal Packages](#internal-packages) section for details.

A package can be **virtual**,
which is similar to virtual modules in OCaml.
A virtual package by default has only the public members as an interface,
but no implementation.
The package can be **implemented** by another package or itself.
See [Virtual Packages](./virtual-pkg.md) for details.

## Module and package naming

All packages must reside in a module.

A module's (fully-qualified) name consists of 2 parts,
the _username_ and the _unqualified name_, separated by a forward slash `/`.
For example, if a module is named `rabbit/containers`,
its username part is `rabbit` (and thus submitted by this user),
and its unqualified name is `containers`.

Legacy modules may consist of more or less than 2 parts in its name,
like `containers` or `rabbit/containers/new`.
Such behavior is not supported for newer modules,
and will be phased out in the future.

The following table shows the module name string and its parts.

| Module name           | Username | Unqualified name | Supported   |
| --------------------- | -------- | ---------------- | ----------- |
| rabbit/containers     | rabbit   | containers       | Yes         |
| containers            | N/A      | containers       | No (legacy) |
| rabbit/containers/new | rabbit   | containers/new   | No (legacy) |

A package's (fully-qualified) name consists of its containing module,
and an unqualified _package path_ within the module.
The package path may be empty, or a path with components separated by a forward slash.
The two parts are separated by a forward slash if the package path is not empty.

Currently, the package's full name is derived from its path and its containing module,
specified in the following [Package discovery](#package-discovery) section.

The following table shows the package's full name in relationship with its module name and path.

| Module name       | Package path | Package full name (derived)   |
| ----------------- | ------------ | ----------------------------- |
| rabbit/containers | (empty)      | rabbit/containers             |
| rabbit/containers | hashmap      | rabbit/containers/hashmap     |
| rabbit/containers | hashmap/raw  | rabbit/containers/hashmap/raw |
| octocat/list      | linked       | octocat/list/linked           |

With legacy modules, packages from different modules may share the same full name.
Currently, no ambiguity is allowed when resolving package names.
If two packages resolves to the same full name when building,
the build system should abort and return an error.

Although technically module and package name components are allowed to contain any character execpt `/`,
we recommend and plan to restrict the character set to ASCII identifiers,
to prevent causing issues on other parts of the toolchain and compiler.

## Package discovery

The `source` field in `moon.mod.json` specifies where package scanning starts,
relative to the folder containing `moon.mod.json`.
Package paths are the relative path (normalized to forward slash) relative to this root path.

If not specified, the package scanning root path is `.`,
meaning the packages are relative to the root of the module.
Newer modules created by `moon new` by default sets this to `src`.

To discover all package within the module,
one recursively search from the scanning root for files named `moon.pkg.json`,
unless the folder contains `moon.mod.json`.
Common non-code folders will be skipped during this process,
such as `.git`, `node_modules` and `target`.

The followings are examples of folder structure and search result,
for common folder layouts with root `.` and root `src`:

```
/
  moon.mod.json       (root of module)
    Assuming { source: ".", name: "rabbit/containers" }

  moon.pkg.json       (package "rabbit/containers")
  linked_list/
    moon.pkg.json     (package "rabbit/containers/linked_list")
  hashmap/
    moon.pkg.json     (package "rabbit/containers/hashmap")
    raw/
      moon.pkg.json   (package "rabbit/containers/hashmap/raw")

  vendor/             (not a package)
    another/          (root of another module, not a package)
      moon.mod.json
      moon.pkg.json
```

```
/
  moon.mod.json       (root of module)
    Assuming { source: "src", name: "rabbit/containers" }

  src/                  (root of package scanning)
    moon.pkg.json       (package "rabbit/containers")
    linked_list/
      moon.pkg.json     (package "rabbit/containers/linked_list")
    hashmap/
      moon.pkg.json     (package "rabbit/containers/hashmap")
      raw/
        moon.pkg.json   (package "rabbit/containers/hashmap/raw")

  moon.pkg.json       (not a package, will not be scanned)
  not-a-pkg/
    moon.pkg.json     (not a package, will not be scanned)
  vendor/             (not a package)
    another/          (not a package)
      moon.mod.json
      moon.pkg.json
```

## Internal packages

MoonBit has the notion of **internal package**s,
which restrict importing in other packages.

A package is internal if its unqualified path contains a component with the special name `internal`.
For example, both `rabbit/containers/internal` and `rabbit/containers/hashmap/internal/raw`
are internal packages.

An internal package may only be imported by another package
that shares the same full name up to (but not containing) the internal component.
An internal package may import any non-internal package.

The following table shows examples of valid and invalid imports regarding internal packages.
Common prefixes are highlighted:

| Importer                  | Dependency                | Can import?                         |
| ------------------------- | ------------------------- | ----------------------------------- |
| **user/pkg**/a            | **user/pkg**/b            | Yes, no internal involved           |
| **user**/another/e        | **user**/pkg/a            | Yes, no internal involved           |
| **user/pkg/a**            | **user/pkg/a**/internal   | Yes, shares common prefix           |
| **user/pkg/a**            | **user/pkg/a**/internal/b | Yes, shares common prefix           |
| **user/pkg/a/internal**/b | **user/pkg/a/internal**/c | Yes, shares common prefix           |
| **user/pkg/a**/internal/b | **user/pkg/a**            | Yes, no internal involved           |
| **user/pkg**/a/internal/b | **user/pkg**/d            | Yes, no internal involved           |
| **user/pkg**/d            | **user/pkg**/a/internal/b | No, no common prefix up to internal |
| **user/pkg**/d/internal/f | **user/pkg**/a/internal/b | No, no common prefix up to internal |
| **user**/another/e        | **user**/pkg/a/internal/b | No, different module                |
