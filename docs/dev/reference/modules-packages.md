# Module and packages

## High-level constructs

A **module** in MoonBit is the unit of dependency version resolution.
The root of a module is signified by `moon.mod` or the legacy `moon.mod.json`.
A module recursively contain all directories and files under its root,
until another module is met.

Commands that discover modules, including `moon check`, warn when a module uses
`moon.mod.json` and suggest `moon fmt` to migrate it to `moon.mod`. When both
files exist, `moon.mod` takes precedence and a single format warning asks the
user to remove the old file. Manifest warnings follow the user-log level
(`--quiet` suppresses them) and are suppressed for dependency cache files under
`.mooncakes`.

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

Named import aliases must be unique within each build target's dependencies.
The special alias `*` marks an import-all declaration rather than a package
name, so multiple dependencies may use it, including in `.mbtx` scripts.
Moon passes each import-all declaration to the compiler, which resolves the
imported names.

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

The unqualified name may include a major-version suffix `/vN`, where `N` is
an integer at least 2 without leading zeroes and matches the version's major
component. For example, `rabbit/containers/v2` uses versions `2.x.y`, including
prereleases such as `2.0.0-rc.1`. A mismatched explicit or registry-selected
version is rejected. For `rabbit/containers/v2`,
the username is `rabbit` and the unqualified name is `containers/v2`.
The suffix is part of the module identity: `rabbit/containers` and
`rabbit/containers/v2` can appear together in a dependency graph, each with
its own version requirements and imports.
Versions 0.x and 1.x use the unsuffixed module name; `/v0` and `/v1` module
suffixes are rejected, including in declarations with no version.

Name/version agreement is checked when constructing a module source, before
manifest mutation or registry acquisition. Local modules may omit their version;
the internal default used for them is not a declared release.

Other name forms, such as `containers` or `rabbit/containers/new`, are legacy.
Local manifests accept names without a username; registry CLI coordinates
require at least a username and a module component. Module-only commands such
as `moon add`, `moon fetch`, and `moon view` also accept legacy module names
with additional components.

The following table shows the module name string and its parts.

| Module name           | Username | Unqualified name | Handling                        |
| --------------------- | -------- | ---------------- | ------------------------------- |
| rabbit/containers     | rabbit   | containers       | Standard                        |
| rabbit/containers/v2  | rabbit   | containers/v2    | Standard; version major is 2    |
| containers            | N/A      | containers       | Legacy; local manifests         |
| rabbit/containers/new | rabbit   | containers/new   | Legacy; module-only coordinates |

A package's (fully-qualified) name consists of its containing module,
and an unqualified _package path_ within the module.
The package path may be empty, or a path with components separated by a forward slash.
The two parts are separated by a forward slash if the package path is not empty.

A package path is a logical path within the module, not a filesystem path.
It is always relative to the module's package scanning root (the `source` field
in `moon.mod.json`, if present), and never includes that root prefix.

Currently, the package's full name is derived from its path and its containing module,
specified in the following [Package discovery](#package-discovery) section.

The following table shows the package's full name in relationship with its module name and path.

| Module name          | Package path | Package full name (derived)   |
| -------------------- | ------------ | ---------------------------- |
| rabbit/containers    | (empty)      | rabbit/containers            |
| rabbit/containers    | hashmap      | rabbit/containers/hashmap    |
| rabbit/containers    | hashmap/raw  | rabbit/containers/hashmap/raw |
| octocat/list         | linked       | octocat/list/linked           |
| rabbit/containers/v2 | hashmap      | rabbit/containers/v2/hashmap  |

Registry package coordinates use `user/module[/vN][/package]`.
For example, `rabbit/containers/v2/hashmap` selects package `hashmap` in
module `rabbit/containers/v2`. An explicit module version follows the suffix:
`rabbit/containers/v2@2.1.0/hashmap`. Commands that accept a version after the
package also accept `rabbit/containers/v2/hashmap@2.1.0`.
An explicit version fixes the module boundary, so
`rabbit/containers@1.0.0/v2/hashmap` selects package `v2/hashmap` in
module `rabbit/containers`.

Packages from different modules may share the same full name. For example,
package `v2` in module `a/b@0.1.0` and the root package in module
`a/b/v2@2.0.0` both have the import path `a/b/v2`. Each can be used separately,
but discovering both in one build produces a duplicate-package-name error.
Versioned registry coordinates select modules; versions are not part of
package import paths.

Although technically module and package name components are allowed to contain any character except `/`,
we recommend and plan to restrict the character set to ASCII identifiers,
to prevent causing issues on other parts of the toolchain and compiler.
Registry module and package components additionally may not contain `#`, `?`,
or `%`, because they have URL syntax meaning. These components also reject
whitespace, control characters, and Unicode directional formatting controls.

## Dependency resolving

Modules are versioned using [SemVer][] (Semantic Versioning),
with the (common) extension of breaking change happens on the first non-zero version component.

Dependencies are currently resolved using the [MVS][] algorithm.
The nature of MVS only allows version ranges with `>=` semantics
(higher versions must be compatible with lower versions).

Dependencies currently accept only caret version syntax in manifests.
During MVS selection, caret requirements are interpreted as lower bounds with
Go-style compatibility buckets:

- For versions below `2.0.0`, all versions below `2.0.0` are treated as compatible.
- For versions `>= 2.0.0`, compatibility is split by major version.

As a result, a requirement like `^0.1.3` can resolve to `0.2.0` if that is the
selected version for the module.

Additionally, due to limitations of the MoonBit compiler,
only one module with a specific name may be present in the resolved dependency graph.
The build system will reject violating graph with an error.

Registry deprecation is advisory and does not affect MVS version selection.
After successful resolution, Moon warns once per selected deprecated registry
module/version, including transitive dependencies. A warning includes the
registry's reason when present and one shortest dependency path for transitive
dependencies. Local and workspace modules are not checked against registry
deprecation. Versions and dependencies visited but discarded by MVS do not
produce warnings. The check assumes the local registry index is up to date and
reuses it without refreshing the index or making additional network requests.
These warnings are User Logs, so `--quiet` suppresses them and
structured commands capture them with their other logs.

[semver]: https://semver.org/
[mvs]: https://go.dev/ref/mod#minimal-version-selection

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
Dot-prefixed directories and common non-code folders such as `node_modules`
and `_build` are skipped during this process. The configured scanning root
itself remains explicit and is still scanned when its name starts with `.`;
the filtering applies to its descendants.

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
