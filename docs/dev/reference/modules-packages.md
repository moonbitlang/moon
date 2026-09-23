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

`moon.pkg` is parsed into ordered DSL entries and converted directly to the
`MoonPkg` package model. The DSL keeps JSON-shaped values for decoding individual
fields; it does not construct an intermediate `MoonPkgJSON`. Legacy
`moon.pkg.json` files use their own `MoonPkgJSON` converter. Both converters share
package-kind resolution, import warnings, formatter defaults, and rule validation.
DSL normalization combines declarations and applies `options(...)` overrides.
The converter then reads the normalized fields without mutating the map,
preserving legacy option aliases and type checks. Unknown options are ignored.

Package settings also have these direct declarations: `proof_enabled`,
`bin_name`, `bin_target`, `max_concurrent_tests`, `regex_backend`, `implement`,
and `overrides` use assignments; `virtual(has_default: ...)` declares a virtual
package. They use the same value types, defaults, and validation as their legacy
options. Each may appear only once. Declaring one both directly and inside
`options(...)` is an error, including when the option uses its hyphenated or
underscore alias. Legacy options remain accepted when no direct declaration is
present. This duplicate rule does not change the existing override behavior of
other declarations such as imports and `formatter`.

The compiler also reads package configuration, so accepting a declaration in
Moon alone does not make it usable with older compilers. Direct declarations
require companion compiler support and formatter support to preserve their
spelling through `moon fmt`; see the [package configuration manual](../../manual/src/package.md).

In `moon.pkg`, multiple unconditional import blocks are combined in source order,
separately for regular, test, and whitebox-test imports. Legacy import fields in
`options(...)` replace all corresponding blocks, including when the replacement
is empty. Both
`test-import`/`test_import` and `wbtest-import`/`wbtest_import`
spellings are accepted; specifying both spellings of the same option is an error.
After these overrides, importing the same package more than once emits one
warning per package and import kind. Duplicate items, aliases, and import-all
flags are retained in source order, preserving existing dependency ordering and
alias precedence. Different import kinds do not produce duplicate warnings.
These manifest warnings are suppressed for dependency manifests in `.mooncakes`.

Named import aliases must be unique within each build target's dependencies.
The special alias `*` marks an import-all declaration rather than a package
name, so multiple dependencies may use it, including in `.mbtx` scripts.
Moon passes each import-all declaration to the compiler, which resolves the
imported names.

An import without an explicit alias uses the package's last path component.
For a module's root package, a major-version suffix is omitted: `a/b/v2`
defaults to `@b`, while `a/b/v2/c` defaults to `@c`. Importing both `a/b` and
`a/b/v2` therefore requires an explicit alias for at least one, such as
`import { "a/b" @b1, "a/b/v2" }`. Moon passes the resolved alias to `moonc`
using `-i <interface.mi>:<alias>`; the full package identity retains `/v2`.
For legacy JSON imports without `import-all` enabled, an empty alias is treated
as an omitted alias.

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
In package coordinates, `v0` and `v1` remain ordinary package components:
`a/b/v1/c` selects package `v1/c` in module `a/b`. An explicit module/version
boundary uses `a/b@1.0.0/v1/c`; `a/b/v1@1.0.0/c` is rejected.

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

Package discovery warns authors about direct child packages named `v2`, `v3`,
and so on in an unsuffixed `user/module`, because their import paths can be
confused with major-version modules. The warning applies only to the selected
project's root modules, including workspace members; registry and local-path
dependencies do not warn their consumers. These packages remain usable and
keep their usual default aliases, such as `@v2`. The warning excludes the
installed standard library and noncanonical suffixes such as `v02` and `vx`,
and follows the user-log level (`--quiet` suppresses it).
As a temporary compatibility exception, the warning is suppressed for
`moonbitlang/core/v128` until that package follows the major-version naming
convention.

Although technically module and package name components are allowed to contain any character except `/`,
we recommend and plan to restrict the character set to ASCII identifiers,
to prevent causing issues on other parts of the toolchain and compiler.
Registry module and package components additionally may not contain `#`, `?`,
or `%`, because they have URL syntax meaning. These components also reject
whitespace, control characters, and Unicode directional formatting controls.

## Module dependency resolution

Module resolution selects concrete module sources and versions and records their
dependency edges in `ModuleDependencyGraph`. `ModuleResolutionConfig` supplies
registry and standard-library policy to the `ModuleResolver`; its working
registry access, manifest caches, and diagnostics belong to
`ModuleResolutionContext`. Failures are reported as `ModuleResolutionError`.
Package imports are resolved separately after package discovery.

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

Rupes Recta represents discovered modules and packages as `DiscoveredProject`,
which contains the `ModuleDependencyGraph`, module directories, and the package-level
`DiscoverResult`. Package-selection helpers use this data without needing a
solved package graph. `DiscoveredProject::resolve_packages` validates imports and adds
`PackageRelations` to produce `ResolvedProject`, using the coverage
setting captured during discovery. `ResolvedProject` pairs those validated
package relationships with the declarations and selected module dependencies
used to resolve them.

`PackageRelations` contains the import graph between package build targets,
virtual-package associations, and derived backend support. `PackageGraphBuilder`
is the temporary working state used to construct those relationships.
`pkg_solve::resolve_packages` reports `PackageResolutionError` for invalid package
relationships.

The outer workflow uses `ProjectPreparationConfig` and `ProjectPreparationError`
because it covers module sync, package discovery, and package resolution.
`sync_module_dependencies` resolves and synchronizes modules;
`prepare_synced_project` discovers packages and resolves their relationships.
`discover_single_file_project` synchronizes and discovers a script's synthesized
project; script commands immediately call `resolve_packages` before selecting
a backend. The CLI's `sync_and_discover_project` and its command-specific
preparation helpers also stop after discovery and return `DiscoveredProject`.
Command paths explicitly invoke `resolve_packages` at the original preparation point, before package
selection, planning, or target-directory locking. This keeps the existing
dependency-error ordering and single package graph.

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
