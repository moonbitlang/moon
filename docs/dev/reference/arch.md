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
  - Downloads and manages the dependencies' files on disk. (`moon install`)
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

In a broad sense, `moon` subcommands follows this order when executing project-build commands.

1. Resolve project layout
   - Discover module root.
   - Sync module-level dependencies into the `.mooncakes` directory when needed.
   - Resolve module-level dependencies from the synced dependency result.
   - Discover packages within modules.
   - Resolve package-level dependencies.
2. Generate build graph based on the intent of the user [^graph]
   - Determine the user intent ("build package X to executable", "check package Y", etc.)
   - Determine the logical artifacts requested by that intent.
   - Select provider actions and expand their transitive artifact requirements.
   - Generate a concrete build graph containing the final commandlines to execute.
3. Execute the build graph
   - Generate metadata files required by compiler actions. Full project checks
     also publish `packages.json` and `index.json` for tooling compatibility
     during migration.
   - For a multi-backend invocation, compose the independently lowered
     Execution Plans into one executor graph, sharing compatible physical
     providers such as package prebuild actions. JSON-formatted checks retain
     each diagnostic's backend through the originating n2 Build ID, so they use
     the same composed graph. Backend actions render their backend together
     with any applicable target kind, such as `(wasm, blackbox test)`;
     package-prebuild actions independently render `(prebuild)` before the
     executor receives either description.
   - Execute the concrete build graph in its executor ([n2][]).
     The executor ensures the graph is executed incrementally, rebuilding only the changed parts.
4. Perform other operations required after build
   - Run the built executable.
   - Run the test/bench executables within test/bench environment and collect results.
   - Promote the results of interface files to source directory.

Implementation-wise:

- Steps 1 and 2 is handled in the RR pipeline's crate.
  You can also see a low-level implementation doc comment at its [entry point][rr_home].
- Step 3, as well as wrappers around step 1 and 2 to adapt to the various subcommands,
  are all located in [the `rr_build` module of the main binary crate][moon_rr_build].
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

[moon_rr_build]: /crates/moon/src/rr_build/mod.rs
[rr_home]: /crates/moonbuild-rupes-recta/src/lib.rs
[n2]: https://github.com/moonbitlang/n2

Rupes Recta executions in one target directory share the n2 database at
`<target-dir>/.moon_db`. Target backend, profile, run mode, and standalone build
phase are represented by concrete output paths and build hashes rather than
separate database files. Separately planned graphs still open and load that
database for each execution; sharing persistent state does not compose the
graphs or reuse one open database handle. The dependency and script graphs of a
standalone build execute sequentially against this database.

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
- `$mooncake_bin` is resolved by the command adapter to a `mooncake_bin_dir`
  path before build planning, so RR planning substitutes an already-computed
  launcher directory instead of deriving it from project layout;
- the target directory is passed into planning/lowering and used for generated
  build files and n2 state; and
- package and module directories come from discovery results, not from later
  path guessing.

Source directory, `.mooncakes` directory, target directory, and optional project
manifest path are user/config facts from project discovery. The synced
dependency result is derived data: it contains the resolved module
relationships and module source directories produced by dependency sync.
`ResolveOutput` should contain resolved
build-model data derived from those inputs, not repeat the captured discovery
paths.

Toolchain and host facts follow the same rule. `moonutil::toolchain` owns facts
about the selected MoonBit toolchain tree, including known tool binaries and
the shipped standard-library artifact layout. Command orchestration decides
whether those facts apply to the current build, then passes the selected facts
forward. In particular, `rr_build` chooses `stdlib_path` from `use_std &&
!is_core`; RR lowering, metadata generation, and `all_pkgs.json` generation
consume an `ArtifactPathResolver` that composes the selected stdlib path with
the target layout instead of rediscovering the installed stdlib. Such facts do
not need to be eager: non-native builds do not resolve native-only OS/toolchain
details. Native-oriented compilation resolves compiler paths before planning
and passes them through the build environment, so planning can select optional
runtime members such as SIMDUTF objects and lowering can consume the same paths
without rediscovery.

Prebuild configuration is another environment-sensitive input. When prebuild
configuration scripts run, `rr_build` captures the process environment
explicitly and passes it to prebuild execution. Commands that skip prebuild
configuration, such as `check`, should not capture that environment just to
construct a build plan. This does not disable package pre-build rules or the
bin-dependencies that provide their tools.

Dependency synchronization is explicit in the normal project path. Command
adapters first call dependency sync, then pass the synced dependency result to
package discovery and package relationship resolution. RR should not hide
dependency downloads or `.mooncakes` directory updates behind a plain
project-resolve call.

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
happens after discovering all relevant modules and packages.
There is currently no explicit module resolving, because there is only one (input) module to work on.

If no input module can be found, some subcommands of `moon` enters a "single-file mode",
which treats the input file (if specified) as the content of a synthesized input module and package.
The concrete declaration of the synthesized module/package is out of scope for this document,
please consult the relevant code for the actual implementation.
Subcommands that do not support single-file mode simply fails with an error.

## Module dependency management

There are two types of dependencies in a module.

- A regular dependency is a dependency that can be accessed from code.
- A **binary dependency** (bin-dep) is a dependency that is used for its executable.
  Bin-deps are declared in `moon.mod.json` under `bin-deps`, which is
  deprecated. New tools should be published as portable Wasm executable
  packages and run with `moonx` instead.
  They are resolved only for the input/root modules themselves: bin-deps of regular
  dependencies are not propagated transitively.
  After dependency sync, direct bin-deps of each workspace root are built and
  installed by invoking `moon tool build-binary-dep` inside the dependency module.
  Registry bin-deps are copied into temporary work directories under the
  project target directory before that command runs. The child command uses
  the temporary directory as its target, copies the runnable artifact into
  private storage under `<project target dir>/__moonbin__`, and then removes
  the work directory. Compilation and nested dependency state therefore do not
  modify the registry source under `.mooncakes`. The compatibility build never
  runs package-level source generation (`pre-build`, `moonlex`, or `moonyacc`);
  distributed packages must already contain their generated outputs.
  Experimental module-level prebuild configuration scripts still run so they
  can provide build configuration such as native link flags. Local bin-deps
  retain their existing in-place build behavior, subject to the same
  package-level prebuild prohibition. The child build resolves the bin-dep's
  regular dependencies but excludes its own bin-deps, preserving the
  non-transitive bin-dep model.

There are two kinds of sources that dependencies come from:

- A **registry dependency** is resolved from the local registry index under
  `$MOON_HOME/registry/index`, which is typically populated from
  `mooncakes.io` by `moon update`. See the
  [Moon home layout](./moon-home-layout.md) for its physical structure.
  It is declared with a version range (written as a version number)
  and later resolved to a concrete version.
- A **local dependency** is fetched from a local path.
  It is declared with a relative path from the module's root directory.

Module dependencies in `moon` are resolved using the [MVS][] algorithm,
the same algorithm that Go used.
MVS resolves each module dependency to the lowest version that satisfies all requirements.
Since MoonBit packages follows [SemVer][],
only caret version syntax is supported when specifying version requirements.
The resolver interprets caret requirements with Go-style compatibility buckets:
versions below `2.0.0` are treated as one compatible set, and versions `>= 2.0.0`
are split by major version.
See details in [Modules and packages][mod-pkg].

Current registry configuration behavior today is:

- `RegistryClient` owns the physical registry lifecycle: searching the remote
  service, synchronizing the Git index and HTTP symbols archive, reading the
  local index, and downloading and verifying HTTP package archives and
  prebuilt wasm assets. Resolver code sees only the narrower `Registry`
  capability.
- The public `Registry` trait contains only metadata queries used by resolution.
  Package source acquisition remains on `RegistryClient`; dependency-source
  tests replace it through a crate-private seam.
- `RegistryConfig.api` controls dynamic registry requests such as search.
  `RegistryConfig.index` controls how `moon update` populates the local
  registry index over Git. `RegistryConfig.download` controls immutable package
  archives, prebuilt Wasm artifacts and checksums, and the default
  `symbols.zip` archive.
- `RegistryClient` constructs those URLs internally; callers do not construct
  or depend on its transport URLs. The optional `RegistryConfig.symbols` URL
  remains a compatibility override for `symbols.zip`.
- Existing `registry`-format configurations and `MOONCAKES_REGISTRY` retain
  their former combined endpoint layout while users migrate to the split
  configuration.
- MVS itself resolves against the local on-disk index and does not consume
  `RegistryConfig` directly.

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
- `BuildRuntimeLib` collects the runtime objects into the single runtime
  library used globally by all consumers in the project. TCC-run lowers this
  node directly from all runtime sources instead.

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
- The n2 adapter projects a selected set of execution actions into concrete
  n2 build nodes.

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
dry-run then use the same Execution Plan adapters as project builds.

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
`moon` does not add extra scheduling logic on top of `n2`.

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

The selector and index identify configuration-scoped Check documents:

```text
_build/packages.json
_build/index.json
_build/<backend>/<profile>/check/packages.json
```

The scoped document retains the complete legacy single-module-oriented shape,
including its top-level `backend` and `opt_level` fields, all resolved packages,
and conditional metadata for all package files. This first migration step only
relocates that document behind the selector; backend/profile projection and
removal of redundant legacy fields are deferred to a follow-up change.

Standalone-file checks similarly publish their selector and scoped document,
and replace the shared backend index:

```text
_build/<filename>.packages.json
_build/index.json
_build/<backend>/<profile>/check/<filename>.packages.json
```

Project checks without a package or path selector publish metadata; focused
project checks leave it untouched. `moon bundle` does not publish these files.
`moon doc` generates only its scoped Check document and passes that path
directly to `moondoc`, without changing the universal selector or backend index
used by the language server.

Scoped documents are atomically replaced before the selector, and the backend
index is replaced last. Readers therefore never follow a newly published index
to a partially written document or stale selector. Byte-identical metadata is
left untouched.
