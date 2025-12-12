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
   - Resolve module-level dependencies.
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

## Project discovery and layout

Currently, a "project" in `moon` is a single input module [^input_module]
and all packages within it.
The meanings and layouts of modules and packages
are described in [Modules and Packages][mod-pkg].
The input module is the module that contains the working directory.
In other words, it is the module represented by the closest ancestor directory (including CWD)
that contains a `moon.mod.json` file.

[mod-pkg]: ./modules-packages.md

[^input_module]:
    The dependency resolver is designed with multiple modules in a project in mind,
    but currently both it and most of the project only supports a single module.
    Since this module (these modules) is the input to the dependency resolver,
    it's called the "input module(s)".

    Some code may refer to it as the "main module",
    but so far there's only one module to work on, so there is no ambiguity yet.

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
  The implementation of binary dependencies is complex. Please check (TBD) for details.

There are two kinds of sources that dependencies come from:

- A **registry dependency** is downloaded from the `mooncakes.io` package registry.
  It is declared with a version range (written as a version number)
  and later resolved to a concrete version.
- A **local dependency** is fetched from a local path.
  It is declared with a relative path from the module's root directory.

Module dependencies in `moon` are resolved using the [MVS][] algorithm,
the same algorithm that Go used.
MVS resolves each module dependency to the lowest version that satisfies all requirements.
Since MoonBit packages follows [SemVer][],
only the caret version range (all compatible versions) is supported when specifying version requirements.
See details in [Modules and packages][mod-pkg].

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
For project-wide subcommands like `moon check`, an intent is emitted for each individual package.
Filtering packages in subcommands operate by only emitting the intents of the target packages,
instead of emitting for every (applicable) package in the project.

The design of intents allows a single intent to be mapped to multiple build-plan nodes,
and also into different node patterns based on the properties of the package.

For example, for a `Check(package)` intent (`moon check -p package`),
it will map into "check package source", "check package whitebox text" and "check package blackbox test".
However, if the package does not contain whitebox test files,
the "check whitebox" node will be omitted.
If the package is virtual, then a list of virtual package checking nodes will be used instead.

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

- Each build-plan node’s command line is chosen based on its own metadata
  (package, backend, build target kind, action) and its dependencies;
- Input/output files are computed via an artifact-translation function and
  attached to the node; and
- Additional inputs (such as source files) may be attached
  to represent files that are not produced by other build-graph nodes.

Each build-plan node is currently mapped to **zero or one** concrete build-graph node.
Lowering a single build-plan node to multiple concrete nodes is not
supported (hence the `index` field in the node declaration).

The concrete rules of lowering is performed in [its module](/crates/moonbuild-rupes-recta/src/build_lower/mod.rs).

The layout of the target directory (the paths of all artifacts)
is defined in [its module](/crates/moonbuild-rupes-recta/src/build_lower/artifact.rs).

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
