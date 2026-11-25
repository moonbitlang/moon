# Architecture and overview

> This document reflects the state of the repository around 2025.12.
> Please use the actual implementation as the ultimate source of truth.

## Scope

MoonBuild, i.e. `moon`, is the build system for MoonBit.

The build system performs the following operations on demand:

- Project discovery.

  - Locates the project root from the given directory.
  - Discovers the package structure.

- Dependency management.

  - Discovers dependency graph from registry.
  - Downloads and manages the dependencies' files on disk. (automatic dependency synchronization)
  - Modifies the dependency list upon request (`moon {add,remove}`).

- Build graph generation and execution.

  - Generate build graph corresponding to build operations (`moon {check,build,run,test,...}`.
  - Execute the build graph incrementally.
  - Renders the raw diagnostics emitted from the compiler.

- Execute build artifacts.

  - Executes artifacts generated for running, testing, and benchmarking.
  - For expect/snapshot tests, report the diff and updates relevant files.
  - For benchmarking, report the benchmark result.

- Facade for other tooling.

  - Generate documentation for a project.
  - Formatting source code.
  - Generating human-readable package public interface files.
  - Upgrading the toolchain.

We will ignore the facades and focus on the core project-building code path
for the rest of this documentation.

## Vocabulary

We will use these terms in the document:

- The **project** is the environment in which you execute `moon`.
  It's not a first-class concept in the code, nor is it used broadly;
  most of the time it simply means "the current local module".

- A **module** is the unit of dependency versioning and management.
  It is analogous to a [Go module](https://go.dev/ref/mod).

- A **package** is the unit of compilation, namespacing and dependency.
  It is analogous to a [Go package](https://go.dev/ref/spec#Packages).
  The concrete definition and layout of modules and packages can be found in
  [Modules and Packages](./modules-packages.md).

- The **build plan** is a logical representation of requested artifacts and
  the actions that produce them. Its actions are either backend build-plan
  nodes (for example, "compile package X" or "check package Y") or package
  prebuild actions. It does not contain the actual command line to execute.
  Actions require logical artifacts; the plan records the action that provides
  each artifact.

- The **execution plan** is the executor-neutral concrete graph. It contains
  commands, input paths, external file observations, and declared physical
  outputs. Execution actions have process-local IDs; physical outputs are
  identified by their concrete paths.

- The **n2 graph** is the [Ninja][]-like executor projection of an execution
  plan. Its node is called a **build node**.

[ninja]: https://ninja-build.org/

## The pipeline

The timing and ownership of configuration selection are recorded in
[ADR 0008](../../adr/0008-select-build-configuration-before-package-resolution.md).

In a broad sense, `moon` subcommands follows this order when executing project-build commands.

1. Resolve project layout
   - Discover module root.
   - Sync module-level dependencies into the `.mooncakes` directory when needed.
   - Resolve module-level dependencies from the synced dependency result.
   - Discover packages within modules.
   - Select the requested Build Configurations: backend (using module preferences when implicit)
     and the command's effective debug/release profile.
   - Resolve complete package relationships for each requested backend; fail if any fails.
2. Generate build graph based on the intent of the user [^graph]
   - Determine the user intent ("build package X to executable", "check package Y", etc.)
   - Prepare compiler options for the selected configuration and consume its backend's
     already-resolved dependency relationship. Do not select the profile again at this stage.
   - Determine the logical artifacts requested by that intent.
   - Select provider actions and expand their transitive artifact requirements.
   - Generate a concrete build graph containing the final commandlines to execute.
3. Execute the build graph
   - For a multi-backend invocation, compose the independently lowered
     Execution Plans into one executor graph, sharing compatible physical
     providers such as package prebuild actions. JSON-formatted checks retain
     each diagnostic's backend through the originating Action ID, so they use
     the same composed graph. Backend actions render their backend together
     with any applicable target kind, such as `(wasm, blackbox test)`;
     package-prebuild actions independently render `(prebuild)` before the
     executor receives either description.
   - Generate metadata files required by compiler actions. Full project checks
     also publish `packages.json` and `index.json` for tooling compatibility
     during migration.
   - Execute the concrete build graph in its executor ([n2][]).
     The executor ensures the graph is executed incrementally, rebuilding only the changed parts.
4. Perform other operations required after build
   - Run the built executable.
   - Run the test/bench executables within test/bench environment and collect results.
   - Promote the results of interface files to source directory.

Implementation-wise:

- `moonbuild::prepare` synchronizes dependencies and prepares script declarations.
  [RR][rr_home] handles package discovery, package resolution, planning, and
  lowering; `moonbuild::compile` prepares external inputs and drives planning
  and lowering.
- `moonbuild::compile::BuildSession` holds the target-directory lock across
  module prebuild configuration, planning, metadata publication, and execution.
  Its execution entry points publish metadata through a private module before
  handing the graph to `moonbuild::execution`. Dry-run planning only locks when
  it runs module prebuild configuration; a dry-run session cannot execute.
- RR's `metadata` module owns the generated package document model. Moonbuild
  serializes and publishes it, and owns the active backend/profile selector.
  These output formats are separate from moonutil's input manifest declarations.
- Moon's private `cli` modules own command policy. `BuildFlags::compile_config`
  adapts command flags, while `cli::selection` owns shared package and backend
  selection. Commands select intents and metadata
  requirements, and keep the session alive while consuming build outputs. Run and test commands release
  it before launching user programs; test promotion and its partial rebuild
  share a new session. Standalone Wasm callers use
  `moonbuild::compile::build_standalone_wasm` without constructing CLI arguments.
- Build results retain the `BuildConfiguration` used by metadata, runtime
  selection, and profiling. Backend-specific compiler settings stay in
  `CompileConfig` and are consumed during planning and lowering; running an
  artifact does not require those compiler settings. The run result takes its target
  directory from the build's artifact path resolver.
- `moonbuild::compile::prepare_backend` captures backend resources, and
  `prepare_config` binds artifact paths to the selected `BuildConfiguration`.
  RR's `CompileConfig` validates matching backend, profile, and operation at
  construction and exposes those selections and paths read-only. `TargetLayout` binds that configuration;
  path methods do not accept a separate backend. Formatting uses `FormatLayout`
  with its existing output directory, without selecting a build configuration.
- Step 4 is handled separately in each individual subcommand.

[^graph]:
    Unless stated otherwise, "build graph generation" refers to the RR pipeline (`moonbuild-rupes-recta`).
    The legacy pipeline (`moonbuild`) eagerly determines the packages to build,
    generates a full Ninja-style command graph for the project
    and relies on the executor to select the subset of commands to run for a given invocation.

    The legacy pipeline does not model logical build nodes or user intent:
    it only materializes concrete commands and manually concatenates paths.
    The "subset of command" is frequently just all commands within the build graph.
    These limit the ability for it to precisely (or sometimes, correctly) determine the commands to run.

[rr_home]: /crates/moonbuild-rupes-recta/src/lib.rs
[n2]: https://github.com/moonbitlang/n2

Rupes Recta executions in one target directory share the n2 database at
`<target-dir>/.moon_db`. Target backend, profile, and run mode are represented
by concrete output paths and build hashes rather than separate database files.
Both ordinary projects and standalone scripts execute one complete graph per
invocation, composing backend plans before n2 adaptation. Dependency ordering
comes from producer edges, with no separate dependency phase.

n2 records completed builds in an append-only log. When the database is at
least 2 MiB, opening it compacts the log if all path records and live build
records fit in one third of its current size. Compaction scans the complete
database after replay, independently of the partial graph that opened it, and
preserves every path record so persistent path IDs remain stable. Moon holds
the target-directory lock across database use because compaction replaces the
database file before returning its append handle.

A later complete build record supersedes an entire older record when they
share any declared output. This whole-record rule is intentionally
conservative for histories whose multi-output ownership changes. For example,
after records for outputs `{a, b}` and then `{a, c}`, compaction may discard
the older `{a, b}` record. If the graph later returns to `{a, b}`, the
invocation that performed compaction may still use the older state replayed
before rewriting, while a subsequent invocation rebuilds once and records
fresh state.

Known n2 limitation: replay validates cached work from command, input, and
output metadata rather than output-producer provenance. If overlapping output
ownership changes and an intervening producer preserves the old output
timestamps, the invocation that opens and compacts the complete history can
reuse an older compatible record and observe stale output. Compaction removes
that older record, so a subsequent invocation rebuilds. Normal `--target all`
and single-target alternation does not create this ownership history:
multi-backend composition shares a provider only when its complete Execution
Action and output set agree, while backend-specific outputs use distinct paths.

### Directory and environment facts

Directory discovery is intentionally front-loaded. Command entry points should
calculate facts such as the selected project root, target directory, workspace
selection, and `.mooncakes` directory once, then pass those facts into later
phases. Later phases should not rediscover them from the working directory.

This is part of the compiler-style shape of the RR pipeline: for directory and
project paths, the command layer captures user input and passes the result
forward instead of letting later phases infer it again. In particular:

- project and workspace selection are captured before package discovery;
- the selected workspace manifest and canonical member list are carried as one
  completed layout, so dependency sync and package discovery do not reopen
  `moon.work`;
- the `.mooncakes` directory is computed during project discovery and passed
  into dependency sync;
- the target directory is passed into planning/lowering and used for generated
  build files and n2 state; and
- package and module directories come from discovery results, not from later
  path guessing.

Standalone-file commands scope this target directory by the input's complete
filename, including its extension. The default is
`<source-dir>/_build/<filename>`; an explicit `--target-dir <dir>` produces
`<dir>/<filename>`. The source-local default separates files in different
source directories, while the complete filename separates differently named
files within one target root. Callers that reuse an explicit target root for
files with the same complete filename also reuse their synthetic package
artifacts, metadata, lock, and n2 database; they must
choose distinct target roots when that isolation is required.

Source directory, `.mooncakes` directory, target directory, and optional project
manifest path are user/config facts from project discovery. The synced
dependency result is derived data: it contains the resolved module
relationships and module source directories produced by dependency sync.
`ResolvedProject` should contain resolved
build-model data derived from those inputs, not repeat the captured discovery
paths.

`DiscoveredProject` contains module resolution and package declarations, which
command adapters use to select packages and target backends. Its `resolve_packages` method
constructs `ResolvedProject` only after every requested backend resolves successfully.
`ResolvedProject.package_relations` maps backends to complete `PackageRelations` values: import
edges, virtual users, implementation mappings, and support derived from active
edges. Errors are returned immediately; unrequested backends are not resolved.
Intent expansion, planning, lowering, metadata, and graph export consume the
selected relationship without further solving or mutation.

Toolchain and host facts follow the same rule. `moonutil::toolchain` owns facts
about the selected MoonBit toolchain tree, including known tool binaries and
the shipped standard-library artifact layout. Command orchestration decides
whether those facts apply to the current build, then passes the selected facts
forward. In particular, `moonbuild::compile` chooses `stdlib_path` from `!is_core`;
RR lowering, metadata generation, and `all_pkgs.json` generation
consume an `ArtifactPathResolver` that composes the selected stdlib path with
the target layout instead of rediscovering the installed stdlib. Such facts do
not need to be eager: non-native builds do not resolve native-only OS/toolchain
details. `moonbuild::compile` captures OS and compiler paths only for Native and LLVM,
and stores them in the selected backend variant of `CompileConfig`. Planning
and lowering borrow that same configuration, so planning can select optional
runtime members such as SIMDUTF objects and lowering can consume the same paths
without rediscovery or a copied planning configuration.

`moonbuild::compile` orchestrates planning and lowering. Once a Build Plan is
complete, it scans the selected toolchain include directory only if the plan
contains C-stub, runtime-object, or generated-C compilation. The scan follows
symlinks and supplies sorted file paths to RR lowering, which attaches them to
those actions as inputs. Native checks and plans without C compilation do not
observe the include tree. This preparation uses the existing compiler paths;
it does not select another toolchain or rebuild the plan.

Prebuild configuration is another environment-sensitive input. When prebuild
configuration scripts run, `moonbuild::compile` captures the process environment
explicitly and passes it to prebuild execution. Commands that skip prebuild
configuration, such as `check`, should not capture that environment just to
construct a build plan. This does not disable package pre-build rules.

Dependency synchronization is explicit in the normal project path. Command
adapters call `moonbuild::prepare::sync_and_discover_project`, which synchronizes
modules through Mooncake and passes their graph and source directories to RR
for package discovery. `ProjectPreparationConfig` owns sync policy and
constructs RR's `DiscoveryConfig` with the same parallelism limit. Commands select
packages and backends from the returned `DiscoveredProject` before resolving
package relationships. RR callers that already have synchronized modules can use
`discover_synced_project` directly with `DiscoveryConfig`, which only carries
coverage and discovery parallelism settings; they do not need registry, workspace,
or download settings.

Single-file preparation also belongs to `moonbuild::prepare`: it reads script
declarations, normalizes versioned imports, consults cached registry metadata
when required, and constructs the synthetic root module. It synchronizes that
root through `mooncake::pkg::sync::sync_roots`, the same module-resolution and
source-acquisition implementation used for ordinary projects and workspaces.
RR receives the synchronized modules, invoked source path, source kind, and
normalized imports, then discovers dependency packages and synthesizes the
single-file package. Module version selection remains in `mooncake`; RR has no
dependency on registry access or downloading.

Mooncake's synchronization interface accepts root declarations and `SyncOptions`
without command-line types. Its `auto_sync` convenience entry loads the selected
project's root declarations and uses project-local dependency storage; single-file
preparation can instead supply a shared source cache. Both paths retain the
installed-core policy. `moon work sync` resolves versions to update manifests
without acquiring dependency sources, so it does not use this acquisition path.

## Project discovery and layout

Currently, most subcommands in `moon` still work on a single input module [^input_module]
and all packages within it.
The meanings and layouts of modules and packages
are described in [Modules and Packages][mod-pkg].

For single-module commands, the input module is the module that contains the working directory.
In other words, it is the module represented by the closest ancestor directory (including CWD)
that contains a `moon.mod` or `moon.mod.json` file.

`moon build`, `moon check`, `moon test`, `moon fmt`, and `moon info` additionally support an
explicit workspace root via `moon.work`. Moon's discovery is order-sensitive:

- Search the current directory and its ancestors for `moon.work` and module
  manifests (`moon.mod` or `moon.mod.json`).
- If a `moon.work` is found before any module manifest, use it.
- If a module manifest is found first, keep it as the current module root and
  continue searching ancestors for `moon.work`.
- An ancestor workspace manifest found after that only applies if it explicitly lists that module.
- Otherwise, fall back to that module manifest.

Unlike Go, Moon does not unconditionally select the nearest ancestor workspace
manifest. Once Moon finds a module boundary, an unrelated ancestor workspace
does not capture that module.

After selecting a workspace, Moon exposes a selected module to member-scoped
commands only when that workspace explicitly lists the module.

If `moon.work` and a module manifest are colocated, the workspace takes
precedence. If the workspace lists `.` as a member, member-scoped commands
select that colocated module. Otherwise, Moon warns that the workspace does not
list its colocated module; workspace-wide commands still use the workspace,
while member-scoped commands cannot infer a target module from that directory.
This warning is a User Log and is suppressed by `--quiet`.

`MOON_WORK` can override this selection:

- unset, empty, or `auto`: use the discovery rules above
- `off`: disable workspace mode and stay in single-module mode
- a path to `moon.work`: pin selection to that workspace

`MOON_NO_WORKSPACE` remains as a deprecated alias for `MOON_WORK=off`.

The workspace manifest is intentionally small. `moon.work` currently supports:

- `members = ["./app", "./lib"]` to list workspace roots.

`preferred_target` in `moon.work` is deprecated. Commands warn when they read
it, but they do not use it for backend selection. `moon fmt` removes it. Set
`preferred_target` in each module manifest instead.

When Moon writes `use` entries, relative paths are normalized with `/` separators. Absolute paths
are kept as absolute OS-specific paths and are not made portable.

[mod-pkg]: ./modules-packages.md

[^input_module]:
    The dependency resolver is designed around a set of input/root modules.
    In Go terms, these are the "main modules"; in this document we usually call
    them "workspace roots" to avoid confusion with executable `main` packages.
    Much of the CLI still assumes there is only one such root, but `moon build`,
    `moon check`, `moon test`, `moon fmt`, and `moon info` now handle multiple
    workspace roots when they come from `moon.work`.

The packages to work with are specified through the command-line arguments.
The accepted formats slightly varies between subcommands due to historical reasons,
but they may be one of these:

- A fully-qualified package name.
- A fuzzy-match against fully-qualified package names.
- A path to a directory containing a package.
- A path to a file within a package.

Resolving the package selectors to their in-memory definition
happens after discovering all relevant modules and packages and before resolving
their package imports. The selected project's root modules bound the default
package scope; explicit selector rules remain command-specific.

If no input module can be found, some subcommands of `moon` enters a "single-file mode",
which treats the input file (if specified) as the content of a synthesized input module and package.
The concrete declaration of the synthesized module/package is out of scope for this document,
please consult the relevant code for the actual implementation.
Subcommands that do not support single-file mode simply fails with an error.

An explicit `.mbtx` file path selects single-file mode for `moon build`,
`moon check`, and `moon test` even when the path is inside an ordinary package.
The `.mbtx` file is therefore the synthetic package's source; it is not
interpreted as a selector for the containing package.
Source interpretation follows the invoked filename, including when a symlink's
target has a different suffix. The canonical file identity still determines
standalone build storage.
The synthetic `.mbtx` package is always executable. Building, running, and
testing a script require a user `fn main`; tests execute the generated test
entrypoint instead of the script's main.
Standalone packages use the same implicit prelude as ordinary packages. Other
packages must be imported explicitly; discovering a module does not import its
packages, even when Markdown front matter lists it in `moonbit.deps`.
Standalone `.mbt` inputs remain supported by `moon check`, `moon test`, and
`moon run`, but emit a deprecation warning directing users to `.mbtx` with front
matter imports. This warning applies only to synthetic standalone packages;
ordinary project `.mbt` files retain their package semantics.

Script declarations use `moon.pkg` grammar in leading front matter fenced by
three or more hyphens, such as `----`:
`import { ... }` declares dependencies and `permission(...)` declares the
embedded Moonrun Policy. The shared front matter parser reads import
structures and untyped permission values; dependency resolution selects module
versions, and Moonrun validates the effective permission schema. An optional
first-line shebang is skipped when interpreting either declaration format.
This is header-parser behavior only; compiler tools receive the original script
unchanged.
Embedded policies use `permission(...)` in front matter; `// policy:` comments
are ordinary comments and do not declare permissions. See
[`moonx`](moonx.md#standalone-mbtx) for syntax and policy precedence.

`moon fmt` also accepts explicit `.mbtx` paths, including several scripts mixed
with package selectors. Each script is formatted on its own without loading
the surrounding project or resolving its dependencies. Scripts use the same
formatter invocation as ordinary source files. `--check` compares the formatted
result with the script and fails if they differ; `--warn` reports differences
without changing the script.
Package-wide formatting continues to exclude `.mbtx` files, and other file
paths continue to select their containing package. Explicit script paths are
validated before formatting begins, and repeated paths to the same script
are formatted once. Script dry runs render input and output paths relative to the
invocation directory, including `../` paths, so scripts in different directories
remain distinguishable. Package plans retain project-relative paths even when
combined with scripts. Paths on different Windows drives remain absolute.

## Module dependency management

Module dependencies provide packages that can be imported from code.

Module version resolution consumes an initialized Module Dependency Graph.
Project preparation validates the roots and, for builds that use the installed
standard library, loads core and registers it before resolution. Building core
itself skips this injection; a workspace mixing core with other roots remains
unsupported. The resolver preserves input root IDs and the supplied standard
library while extending the graph with selected dependencies. It does not
locate the installed toolchain or reload its core manifest. Registry metadata
and local dependency manifests are still read during dependency traversal.

Binary dependency execution has been removed. Nonempty `bin-deps` declarations
in input/root modules are rejected during module resolution, before dependency
acquisition. Empty declarations are accepted. Old dependency manifests
may still contain `bin-deps`; these declarations are ignored, preserving the
previous non-transitive behavior. `moon add --bin`, the internal
`moon tool build-binary-dep` command, and the `$mooncake_bin` prebuild placeholder
have also been removed.

Remove `bin-deps` from the root manifest and run portable Wasm tools with
`moonx` instead. Pin the tool version in the command. Tools without published
executable assets need to publish them before consumers can migrate. The
`bin-name` and `bin-target` package metadata fields have also been removed;
existing entries follow the unknown-key warning policy.

There are two kinds of sources that dependencies come from:

- A **registry dependency** is resolved from registry metadata. By default this
  is the local index under `$MOON_HOME/registry/index`, typically populated from
  `mooncakes.io` by `moon update`; an opt-in HTTP backend fetches exact release
  metadata on demand. See the
  [Moon home layout](./moon-home-layout.md) for its physical structure.
  It is declared with a concrete version whose dependency metadata must exist
  in the registry. MVS may later raise the selected version to satisfy another
  dependency's requirement.
- A **local dependency** is fetched from a local path.
  It is declared with a relative path from the module's root directory.

Module dependencies in `moon` are resolved using the [MVS][] algorithm,
the same algorithm that Go uses.
The resolver traverses the exact declared versions and then selects the highest
required version in each compatible set. Missing declared versions are errors;
the resolver does not search for a higher available release to replace them.
Manifests specify full [SemVer][] version numbers, not version ranges.
The resolver uses Go-style compatibility buckets:
versions below `2.0.0` are treated as one compatible set, and versions `>= 2.0.0`
are split by major version.
See details in [Modules and packages][mod-pkg].

Current registry configuration behavior today is:

- `RegistryClient` selects either `GitIndex` or `HttpMetadata` for metadata
  queries. HTTP storage is encapsulated by its internal `MetadataCache`, while
  Git keeps its parsed index cache. The client also
  searches the remote service and downloads and verifies package archives and
  prebuilt wasm assets. Resolver code sees only the narrower `Registry`
  capability.
- The public `Registry` trait exposes exact-version metadata and two distinct
  latest operations. `cached_latest_version` reads the locally known answer
  without network access; `None` means no cached answer, not remote absence.
  `query_latest_version` contacts the registry and retains the answer; it must
  not silently fall back to a stale version. The Git client implements the
  latter by synchronizing its index before reading it. Metadata refreshes do
  not update `symbols.zip`; Git-mode `moon update` explicitly synchronizes the
  index and symbols separately. A symbols failure warns without failing the
  index update. MVS and deprecation checks read exact releases.
  `resolve_cached_unversioned_path` resolves import
  paths using cached latest answers only. The complete Git version map is an
  internal cache, not a public cache trait. Queries preserve registry read
  failures instead of reporting them as missing modules or cache misses.
  Package source acquisition remains on `RegistryClient`; dependency-source
  tests replace it through a crate-private seam.
- Command inputs become a `ModuleRequest::Versioned` or `Unversioned` request.
  Commands separately decide whether remote access is allowed; an unversioned
  request does not itself choose current or cached latest.
- `moonx` uses cached latest for unpinned coordinates and
  queries current latest on a cache miss or for explicit `@latest`. `moon add`,
  `moon fetch`, and registry binary installation query current latest when no
  version is specified. If metadata cannot be refreshed, these three commands
  warn and explicitly fall back to a cached latest answer, if any; authoritative
  absence and invalid metadata do not permit fallback. This policy belongs to
  the CLI, while the registry query itself remains strict. For Git, refresh
  failures include local synchronization errors; errors reading or validating
  module metadata records still propagate.
  Pinned add/fetch/install refresh the specified version through
  `RegistryClient::refresh_version_info`: Git synchronizes the index, while HTTP
  fetches that exact release and updates only advisory status if already cached.
  On an unavailable registry, the CLI warns and uses `cached_version_info` only
  if that exact release exists locally. Other errors propagate.
  `add/fetch --no-update` skips explicit metadata refresh and uses cached latest
  for unversioned requests. Exact records missing during resolution can still be
  fetched, as can missing sources; this flag is not an offline mode. Frozen
  dependency resolution permits neither metadata nor source cache misses.
- `moon add --upgrade` permits changing an existing dependency's version; a new
  dependency can be added with or without the flag. It does not control metadata
  queries or dependency installation. Both additions and version changes resolve
  and acquire dependencies before writing the manifest. An existing dependency
  without `--upgrade`, or one already at the requested version, is left unchanged.
- `RegistryConfig.api` controls dynamic registry requests such as search.
  `RegistryConfig.index` controls how `moon update` populates the local
  registry index over Git. `RegistryConfig.download` controls immutable package
  archives, prebuilt Wasm artifacts and checksums, and the default
  `symbols.zip` archive.
- `RegistryClient` constructs those URLs internally; callers do not construct
  or depend on its transport URLs. The optional `RegistryConfig.symbols` URL
  remains a compatibility override for `symbols.zip`.
- Dynamic registry API reads share optional Bearer authentication from the
  saved login bound to that API base URL, regardless of module visibility.
  Download clients remain anonymous. See [Moon home layout](./moon-home-layout.md)
  for credential compatibility and redirect boundaries. This does not add
  authenticated metadata fallback to the local-index resolver.
- Existing `registry`-format configurations and `MOONCAKES_REGISTRY` retain
  their former combined endpoint layout while users migrate to the split
  configuration.
- MVS requests every declared dependency's exact version through
  `Registry::version_info`, without constructing transport URLs or enumerating
  version histories. The default adapter still reads the local Git index.
  `MOON_REGISTRY_API=1` enables the experimental HTTP metadata adapter globally,
  including modules already present in that index. The initial HTTP adapter
  accepts public metadata only and sends no login credentials for resolver
  requests. The configured server must support public metadata.
  See [Moon home layout](./moon-home-layout.md) for caching and frozen behavior.

[semver]: https://semver.org/
[mvs]: https://go.dev/ref/mod#minimal-version-selection

## Package and package dependency management

A package is, as mentioned earlier, the unit of compilation.

The compilation of a package is controlled by a number of axes:

- The **target backend** is the platform to build to: WASM, JS, Native, etc.
- The **build target kind** determines how and which part of the package is built:
  _Source_ is the library represented by the package itself.
  The rest are tests: _whitebox test_, _blackbox test_, _inline test_.
  A **build target** is the package combined with its build target kind
  ("package X's blackbox test").
- The backend action, or **build plan node**, to execute on the package:
  build, check, link, etc.
- The properties of the package itself.
  For example, a package can optionally be _virtual_ to be overridable.

The detailed description of these concepts can be found in [Modules and packages][mod-pkg].

The dependency between packages is resolved after the module dependency relationship is resolved.

Each package has an import (dependency) list that applies to all its build targets.
Additionally, whitebox tests and blackbox tests have their own list of dependencies.
Together, these imports determine the package-level dependency edges in the resolved graph
and, by extension, between build-plan nodes.

Import blocks may have [backend conditions](cond-comp.md#conditional-package-imports).
Package solving validates each requested backend's full `PackageRelations` before
returning `ResolvedProject`. Module resolution and package discovery run once;
virtual-package relationships belong to each backend's relationship alongside
its import graph.

Main packages are being tightened relative to ordinary packages:

- Release N warns when a package depends on a main package.
- Release N+1 will reject dependencies on main packages.
- Release N also warns when a main package still relies on blackbox-test-only
  inputs; release N+1 will stop generating blackbox test targets for main
  packages.

This follows the intended model that a main package is an entrypoint, not a
reusable library package.

Each dependency of a package must either be:

- in the same module as the package itself, or
- from a (direct) dependency of the containing module of the package.

In particular,
a package cannot import packages from its module's _transitive dependency_ [^transitive].

[^transitive]:
    This has been allowed in the legacy pipeline for historical reasons.
    It's currently a hard error in the RR pipeline.

The dependency relationship between build targets is captured in
[the package dependency graph](/crates/moonbuild-rupes-recta/src/pkg_solve/mod.rs).

## User intent

The RR pipeline uses the **user intent** as an intermediate layer between the
CLI subcommand and the Build Artifact requests passed to Build Plan construction.

User intents are the normalized, high-level constructs that allow CLI
subcommands to describe the results they want from packages without committing
to the provider actions that produce those results.

User intents are specified on individual packages.
For project-wide subcommands like `moon check` and `moon build`, an intent is
emitted for each individual package.
Filtering packages in subcommands operate by only emitting the intents of the target packages,
instead of emitting for every (applicable) package in the project.

The design of intents allows a single intent to request multiple artifacts,
and also different artifact sets based on the properties of the package.

For example, for a `Check(package)` intent (`moon check -p package`),
it will map into "check package source", "check package whitebox text" and "check package blackbox test".
However, if the package does not contain whitebox test files,
the whitebox Check MI artifact will be omitted.
If the package is virtual, its virtual-contract MI artifact is requested instead.

For `moon check` and `moon build` without an explicit `--target`, CLI planning
may first split selected packages into multiple backend groups using
`module preferred_target -> default backend`,
then emit intents separately for each backend group.

For `moon build`, private `cli/build/selection.rs` completes one `BuildRequest`
per backend using only package declarations. Explicit selectors request their
selected packages; a default build requests linkable packages, or all supported
non-stdlib packages in its scope when none are linkable. If all implicit
preferred-backend groups become empty, selection chooses the shared module
preference or default backend and reselects its supported local packages. It
retains that fallback plan even when empty. An empty explicit selection does
not trigger this fallback.

The build workflow resolves all requested backend imports before planning any
request, then passes these completed requests to the build session. It does not
reinterpret paths, package names, or default scope after package resolution.

This mapping is also on a migration path for main packages: release N keeps the
current artifacts so warnings can be surfaced, while release N+1 will omit
blackbox check/test artifacts for `is-main` packages.

The details of how a user intent is mapped to requested artifacts
is described in [its module](/crates/moonbuild-rupes-recta/src/intent.rs).

## Build Plan actions

`BuildPlan` composes a backend subplan and a package-prebuild subplan.
`BuildPlanActionKey` makes the two top-level action kinds explicit:

- `Backend(BuildPlanNode)` identifies backend-specific semantic work.
- `PackagePrebuild(PackagePrebuildKey)` identifies package-level file
  generation independent of backend target kind.

`TargetKind` is nested only in backend nodes whose meaning varies across
source, whitebox, blackbox, or inline-test targets. It is not an optional field
on a generic action and does not apply to package prebuild.

### Backend build-plan nodes

**Build plan nodes** are logical representation of the command to be executed.
Many build plan nodes operate on build targets,
but nodes that do not directly work on MoonBit source files may have a different shape.
Here are some examples:

- `Check(BuildTarget)` performs check on the given build target.
- `BuildCore(BuildTarget)` builds the given build target to an intermediate format (CoreIR).
- `LinkCore(BuildTarget)` links all dependencies of a build target into the compiled form.
- `BuildVirtual(PackageId)` builds the virtual package interface of the given package
  (build targets don't make sense on virtual packages).
- `BuildRuntimeObject(u32)` compiles one shipped runtime C translation unit.
- `BuildRuntimeLib` collects the runtime objects into the single static runtime
  library used globally by all consumers in the project.

The full list of backend build-plan nodes is available in
[its module](/crates/moonbuild-rupes-recta/src/model.rs). Package prebuild keys
and actions are defined in
[`package_prebuild.rs`](/crates/moonbuild-rupes-recta/src/build_plan/package_prebuild.rs).

### Action dependency

Build Plan actions require artifacts to form a directed acyclic graph. The
artifact registry selects the provider action for each
requirement.

For example, if build target A depends on build target B,
then `Check(A)` must first obtain the public interface of B,
and therefore depends on `Check(B)`.

### Generating the build plan

The build plan in the pipeline is the transitive dependency closure of the
provider actions selected for artifacts translated from user intents.

To generate this build plan, we start from the requested artifact list, select
each artifact's provider action, and iteratively add providers for every new
artifact requirement until no more actions are added.

This process of adding dependencies has the following properties:

- Local. The dependency of each build plan node is only determined from
  the global config and its own metadata.
- Monotonic. The process never deletes planned actions or artifact requirements.
- Terminating. Because the dependency graph is finite, there can only be a finite number of nodes.

The concrete rules of adding dependencies is available in [its module](/crates/moonbuild-rupes-recta/src/build_plan/mod.rs).
You may also consult the [How a package is built](./build.md) page for a closer view of the rules.

## Lowering to the execution plan

The build plan is only a logical description of the build. It is lowered into
a concrete, executor-neutral **execution plan**. Each execution action carries:

- a command line to execute,
- input paths and external file observations,
- declared physical outputs, and
- execution metadata such as cwd, environment, diagnostics, and cache policy.

During lowering:

- Each build-plan node’s command line is chosen based on its own metadata
  (package, backend, build target kind, action) and its dependencies.
- Build Artifact requirements are resolved to their provider actions and
  physical output paths before the execution plan is finalized.
- Additional inputs (such as source files) may be attached to represent files
  that are not produced by another execution action.
- Each execution action receives a process-local `ActionId`; each declared
  output is registered by its concrete path.
- The private `moonbuild::execution::n2` module adapts the complete
  Execution Plan to n2 build nodes. Execution callers select roots by output path.

Each semantic action currently maps to exactly one execution action and n2
build node.
Backend differences are represented while planning the provider action rather
than by dropping no-op actions during lowering. Lowering a single action to
multiple concrete nodes is not supported (hence the `index` field in the node
declaration).

The concrete rules of lowering is performed in [its module](/crates/moonbuild-rupes-recta/src/build_lower/mod.rs).

Lightweight commands that do not select logical Build Artifacts do not need a
synthetic Build Plan. `moon fmt` performs its lightweight project discovery and
constructs complete execution actions directly. Formatter execution and
dry-run then use the same execution and plan-rendering entry points as project builds.

Legacy manifest migration is one formatter execution action. Its internal
formatting and legacy-file removal are encapsulated by the internal
`migrate-manifest` tool command rather than represented as separate execution
actions or a fake removal output.

The layout of the target directory (the paths of all artifacts)
is defined in [its module](/crates/moonbuild-rupes-recta/src/target_layout.rs).

## Execution of the build graph

For current builds, the execution plan is adapted to an n2 graph and handed to [n2][],
which executes it in the usual Ninja-style way:
incrementally (skipping up-to-date nodes)
and with maximal parallelism subject to dependencies and its job limits.
`moonbuild` does not add extra scheduling logic on top of `n2`.

The private `moonbuild::execution::n2` module owns n2 adaptation, the `.moon_db`
location and database access, root lookup, scheduling, and progress capture.
Its parent `execution` module owns `BuildConfig`, `BuildInput`, execution entry
points, and action failure classification. Its private `diagnostics` module
owns compiler diagnostic decoding, generated-driver path rewriting, filtering,
deduplication, limits, and counts; the nested `render` module owns source lookup,
source-map interpretation, and terminal rendering. Only `DiagnosticLevel` is
exposed alongside the execution configuration; the compiler wire model and
source caches remain private. Shared error-code documentation stays in
`moonutil::error_code_docs` for both rendering and `moon explain`.
`BuildInput` keeps the Execution Plan and
action-backend metadata together through composition. The CLI translates its
flags into `BuildConfig`; execution does not depend on CLI parsing types.
Shared artifact paths remain in Rupes Recta's `target_layout` module.
Dry-run and planner snapshots traverse the Execution Plan directly, without
constructing an n2 graph or copying command arguments into a second map.

Captured action output retains only its Action ID, executor termination, and
output. The capture borrows its owning `BuildInput`, keeping the Execution Plan
alive through result processing. The n2 callback maps its Build ID to the
Action ID; it does not copy action metadata, classify diagnostics, or construct
failure messages. Result processing resolves the Target Backend, output kind,
description, and command from that input. It classifies compiler errors from
the original output before diagnostic filtering.
Structured execution results include these failures even when compiler
diagnostics already explain them or a diagnostic limit hides those diagnostics.
Failure messages remain a separate projection, with visibility controlled by
UserLog; their absence does not imply that an action succeeded. The current n2 API
collapses nonzero exits, most abnormal terminations, and per-task execution
errors into the same failure result, so these records cannot distinguish
ordinary compiler errors from a compiler that emitted diagnostics and then
crashed.

Human-readable diagnostic rendering shares source text, character-based line
offsets, and Ariadne sources across each diagnostic batch. Source maps are
also loaded once per batch, including cached misses. Source loading keeps the
existing patch-file fallback when a diagnostic's file is missing on disk, and
caches the resolved source for the rest of the batch.
Compiler locations use one-based Unicode scalar columns and LF-delimited
lines; converting them to Ariadne character offsets uses the cached line
offsets rather than scanning the source for each span. Each batch creates a
fresh cache so later builds, including watch-mode rebuilds, observe file edits.

The target-directory lock still spans mutable preparation, build execution,
and the command's protected result collection. Database encapsulation does not
shorten that lifetime; test and program execution retain their existing unlock
points.

## Artifacts handling

`moon` may perform additional operations on the artifacts generated during the build.
The requested results are keyed by `ArtifactKey` before build-plan construction;
lowering realizes them into physical paths. Callers therefore select results by
logical identity rather than by provider action kind or position in an output
vector. Physical outputs required only for execution, such as a companion dSYM
directory, are not caller-requested results; the current n2 graph still executes
their producer when the output is an unconsumed graph root.

Package metadata is shared with IDE tooling. The universal `packages.json` is
an exact two-field selector containing the active Target Backend and build
profile from the last full Check plan:

```json
{
  "backend": "native",
  "opt_level": "debug"
}
```

The companion `index.json` is a JSON array of every Target Backend for which
that Check invocation planned and published package metadata:

```json
[
  "js",
  "native"
]
```

This set comes directly from the already-planned Check runs, so explicit
targets, module preferences, and package `supported_targets` filtering are
reflected without an additional discovery or resolution pass. Entries are
deduplicated and ordered by Target Backend.

A full Check publishes a package inventory under each selected configuration's
artifact root:

```text
_build/packages.json
_build/index.json
_build/<artifact-dir>/<profile>/check/packages.json
```

The artifact directory normally has the backend name. A Native check across the
Windows boundary uses `native-windows` or `native-unix`, so its package inventory
can coexist with a host inventory under `native`. The selector and index still
report the backend as `native`. IDE clients that look up inventories by backend
read the host inventory if present. A fresh cross build has no inventory at
the host path.

The scoped document retains the complete legacy single-module-oriented shape,
including its top-level `backend` and `opt_level` fields, all resolved packages,
and conditional metadata for all package files. This first migration step only
relocates that document behind the selector; backend/profile projection and
removal of redundant legacy fields are deferred to a follow-up change.

Standalone-file checks publish the same relative metadata layout within their
filename-scoped target directory:

```text
_build/<filename>/packages.json
_build/<filename>/index.json
_build/<filename>/<artifact-dir>/<profile>/check/packages.json
```

Project checks without a package or path selector publish metadata; focused
project checks leave it untouched. `moon bundle` does not publish these files.
`moon doc` generates only its scoped Check document and passes that path
directly to `moondoc`, without changing the universal selector or backend index
used by the language server.

Scoped documents are atomically replaced before the selector, and the backend
index is replaced last. Byte-identical metadata is left untouched.

The private `moonbuild::metadata` module owns this publication sequence,
including the compiler's `all_pkgs.json` for each plan. The command layer selects
`MetadataOutput::Check` for a full check; `BuildSession` publishes it under the
same lock as execution. `moon doc` selects `MetadataOutput::Packages` to publish
its scoped document without changing the selector or index.
