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

- The **build plan** is a logical representation of the commands to run.
  It contains **build plan nodes** that represents build operations
  (e.g. "compile package X", "check package Y"),
  but not the actual command line to execute.
  Nodes may depend on the output files of other nodes.

- The **build graph** is a concrete, [Ninja][]-like graph that contains
  the final command lines and input/output file paths to execute.
  The node within it is called a **build node**.

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
   - Determine the initial set build graph nodes (= commands to run) corresponding to the intent.
   - Expand the build graph according to rules into containing all transitive dependencies of these nodes.
   - Generate a concrete build graph containing the final commandlines to execute.
3. Execute the build graph
   - Generate metadata files required for the IDE and compilers to use.
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
    Unless stated otherwise, "build graph generation" refers to the RR pipeline
    (`moonbuild-rupes-recta`). The pre-RR generator used to live in the crate
    named `moonbuild`, but that generator has been removed.

    The `moonbuild` crate still exists as a shared support crate for execution,
    dry-run rendering, build scripts, expect/snapshot support, and several CLI
    utilities. Do not read the crate name as evidence that a selectable legacy
    graph-generation backend still exists.

    Historically, the legacy pipeline eagerly determined the packages to build,
    generated a full Ninja-style command graph for the project, and relied on
    the executor to select the subset of commands to run for a given invocation.
    It did not model logical build nodes or user intent: it materialized
    concrete commands and manually concatenated paths. The "subset of command"
    was frequently just all commands within the build graph. These limitations
    motivated the RR model described in this document.

[moon_rr_build]: /crates/moon/src/rr_build/mod.rs
[rr_home]: /crates/moonbuild-rupes-recta/src/lib.rs
[n2]: https://github.com/moonbitlang/n2

### Directory and environment facts

Directory discovery is intentionally front-loaded. Command entry points should
calculate facts such as the selected project root, target directory, workspace
selection, and `.mooncakes` directory once, then pass those facts into later
phases. Later phases should not rediscover them from the working directory.

This is part of the compiler-style shape of the RR pipeline: for directory and
project paths, the command layer captures user input and passes the result
forward instead of letting later phases infer it again. In particular:

- project and workspace selection are captured before package discovery;
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
not need to be eager: non-native builds should not resolve native-only
OS/toolchain details unless a lowering path actually asks for them.

Prebuild configuration is another environment-sensitive input. When prebuild
configuration scripts run, `rr_build` captures the process environment
explicitly and passes it to prebuild execution. Commands that skip prebuild,
such as `check`, should not capture that environment just to construct a build
plan.

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
that contains a `moon.mod.json` file.

`moon build`, `moon check`, `moon test`, `moon fmt`, and `moon info` additionally support an
explicit workspace root via `moon.work`, following the same discovery precedence as Go workspaces:

- Search the current directory and its ancestors for `moon.work` and `moon.mod.json`.
- If a `moon.work` is found before any `moon.mod.json`, use it.
- If a `moon.mod.json` is found first, keep it as the current module root and continue searching
  ancestors for `moon.work`.
- An ancestor workspace manifest found after that only applies if it explicitly lists that module.
- Otherwise, fall back to that `moon.mod.json`.

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
  Bin-deps are declared in `moon.mod.json` under `bin-deps`.
  They are resolved only for the input/root modules themselves: bin-deps of regular
  dependencies are not propagated transitively.
  After dependency sync, direct bin-deps of each workspace root are built and
  installed by invoking `moon tool build-binary-dep` inside the dependency module.
  Registry bin-deps are copied into temporary work directories under the
  project target directory before that command runs. The child command uses
  the temporary directory as its target, copies the runnable artifact into
  private storage under `<project target dir>/__moonbin__`, and then removes
  the work directory. Compilation, pre-build outputs, and nested dependency
  state therefore do not modify the registry source under `.mooncakes`. Local
  bin-deps retain their existing in-place build behavior. The child build
  resolves the bin-dep's regular dependencies but excludes its own bin-deps,
  preserving the non-transitive bin-dep model.

There are two kinds of sources that dependencies come from:

- A **registry dependency** is resolved from the local registry index under
  `~/.moon/registry/index`, which is typically populated from `mooncakes.io`
  by `moon update`.
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

- `RegistryConfig` currently affects how `moon update` populates the local
  registry index.
- MVS itself resolves against the local on-disk index and does not consume
  `RegistryConfig` directly.
- Package artifact downloads and symbols download are still tied to the default
  Mooncakes endpoints.

Future custom/private registry work may configure these pieces together,
rather than reintroducing unused registry parameters into resolve entry points.

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
- The action, or **build plan node** to execute on the package:
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

The RR pipeline uses the **user intent** as an intermediate layer
between the CLI subcommand and the build-plan nodes.

User intents are the normalized, high-level constructs
that allows CLI subcommands to describe the action they want to perform on packages,
without committing to the details of which nodes to use.

User intents are specified on individual packages.
For project-wide subcommands like `moon check` and `moon build`, an intent is
emitted for each individual package.
Filtering packages in subcommands operate by only emitting the intents of the target packages,
instead of emitting for every (applicable) package in the project.

The design of intents allows a single intent to be mapped to multiple build-plan nodes,
and also into different node patterns based on the properties of the package.

For example, for a `Check(package)` intent (`moon check -p package`),
it will map into "check package source", "check package whitebox text" and "check package blackbox test".
However, if the package does not contain whitebox test files,
the "check whitebox" node will be omitted.
If the package is virtual, then a list of virtual package checking nodes will be used instead.

For `moon check` and `moon build` without an explicit `--target`, CLI planning
may first split selected packages into multiple backend groups using
`module preferred_target -> default backend`,
then emit intents separately for each backend group.

This mapping is also on a migration path for main packages: release N keeps the
current nodes so warnings can be surfaced, while release N+1 will omit blackbox
check/test nodes for `is-main` packages.

The details of how an user intent is mapped to build plan nodes
is described in [its module](/crates/moonbuild-rupes-recta/src/intent.rs).

## Build plan node

**Build plan nodes** are logical representation of the command to be executed.
Many build plan nodes operate on build targets,
but nodes that do not directly work on MoonBit source files may have a different shape.
Here are some examples:

- `Check(BuildTarget)` performs check on the given build target.
- `BuildCore(BuildTarget)` builds the given build target to an intermediate format (CoreIR).
- `LinkCore(BuildTarget)` links all dependencies of a build target into the compiled form.
- `BuildVirtual(PackageId)` builds the virtual package interface of the given package
  (build targets don't make sense on virtual packages).
- `RunPrebuild(PackageId, u32)` runs the prebuild command of a package with the given index.
- `BuildRuntime` builds a single runtime artifact that is used globally (in this project) for all users who need it.

The full list of build plan nodes are available in [its module](/crates/moonbuild-rupes-recta/src/model.rs).

### Node dependency

Build plan nodes depend on each other to form a directed acyclic graph called the **build plan**.
The edge represents one node's dependency on the artifact produced by another node.

For example, if build target A depends on build target B,
then `Check(A)` must first obtain the public interface of B,
and therefore depends on `Check(B)`.

### Generating the build plan

The build plan in the pipeline is the transitive dependency closure
of a list of initial nodes that were translated from the user intents.

To generate this build plan, we start from the initial node list,
and iteratively add the dependency of every new node generated,
until no more nodes are added.

This process of adding dependencies has the following properties:

- Local. The dependency of each build plan node is only determined from
  the global config and its own metadata.
- Monotonic. The process never deletes nodes, although it may coerce node to other types.
- Terminating. Because the dependency graph is finite, there can only be a finite number of nodes.

The concrete rules of adding dependencies is available in [its module](/crates/moonbuild-rupes-recta/src/build_plan/mod.rs).
You may also consult the [How a package is built](./build.md) page for a closer view of the rules.

## Lowering to the build graph

The build plan is only a logical description of the build. It must be lowered to
a concrete **build graph** that contains executable commands.

The build-graph data structure comes from the [n2][] crate, a Rust
reimplementation of [Ninja][]. Each node in this graph carries:

- a command line to execute,
- a list of input file paths, and
- a list of output file paths.

During lowering:

- The build plan is first viewed as a `BuildActionPlan`, where each surviving
  build-plan node has a stable action id and hydrated action metadata.
- Each action’s command line is chosen based on its own metadata
  (package, backend, build target kind, action) and its dependencies.
- Logical products are resolved to input/output files and attached to the
  concrete build node.
- Additional inputs (such as source files) may be attached to represent files
  that are not produced by other build-graph nodes.

Each action is currently mapped to **zero or one** concrete build-graph node.
Lowering a single action to multiple concrete nodes is not supported (hence the
`index` field in the node declaration).

The concrete rules of lowering is performed in [its module](/crates/moonbuild-rupes-recta/src/build_lower/mod.rs).

The layout of the target directory (the paths of all artifacts)
is defined in [its module](/crates/moonbuild-rupes-recta/src/target_layout.rs).

## Execution of the build graph

Once lowered, the build graph is handed to [n2][],
which executes it in the usual Ninja-style way:
incrementally (skipping up-to-date nodes)
and with maximal parallelism subject to dependencies and its job limits.
`moon` does not add extra scheduling logic on top of `n2`.

## Artifacts handling

`moon` may perform additional operations on the artifacts generated during the build.
The list of artifacts is generated by the build graph lowering process,
one item per input build plan node.

`packages.json` is the legacy metadata file shared with IDE tooling.
Its top-level shape remains single-module-oriented for compatibility.
