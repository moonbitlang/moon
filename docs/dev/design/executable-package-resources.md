# Executable package resources

## Status

Accepted and partially implemented. The initial `moon build` and `moon run`
slice of the Milestone B development mappings is implemented for executable
source packages. Runtime lookup, the standard-library wrapper, `moon test` and
`moon bench` integration, and distribution layouts remain future work.

The design intentionally fixes only the source declaration and runtime lookup
contract needed by a single executable package. Distribution formats,
development-service orchestration, and relationships between multiple
executables remain future work.

## Problem

MoonBit programs need a stable way to locate read-only files shipped with an
executable, such as templates, schemas, images, and other application data.
Using the process working directory is unreliable:

- `moon run` may be invoked from different directories;
- a built artifact may be launched directly;
- an installed executable and its data commonly live in different directories;
- a macOS application bundle has a platform-defined resource location; and
- future distribution formats may rearrange artifacts without changing source
  code.

Configuration is limited to the source boundary; development, runtime, and
distribution layouts remain convention-driven:

1. Declare one package-relative data directory on the executable package.
2. Use one runtime API to request a resource by name.
3. Run the same program during local development, from a build artifact, or
   from a packaged layout without changing source code.

In this document, *application resources* means package-owned files shipped
with a program. This is unrelated to moonrun's existing **Resource** term for
host-owned files, sockets, and other OS objects.

## Decision summary

The MVP has the following contract:

- Only an executable package owns application resources.
- It declares its source data directory with
  `options(data_dir: "<relative-path>")` in `moon.pkg`.
- `data_dir` is a portable package-relative path. A declared directory must
  exist and be a directory.
- Without `data_dir`, a package has no application resources; a directory
  named `resources` has no implicit meaning.
- A runtime wrapper accepts one relative resource name and either returns an
  existing absolute host path or reports an error.
- The public wrapper is `@env.resource_path`. The first implementation
  guarantees Wasm and Wasm-GC programs hosted by moonrun; other backends keep
  the same API shape but report that the backend is unsupported.
- Runtime lookup is anchored to the MoonBit program artifact, not the working
  directory and not a host runtime such as `moonrun`.
- A runtime instance selects one resource root before resolving requested
  names. It does not fall through to another root when a file is missing from
  the selected root.
- Local `moon build`, `moon run`, `moon test`, and `moon bench` outputs map the
  source data directory to the same configured relative location beside each
  applicable executable artifact. Unix uses a symbolic link and Windows uses
  an NTFS junction.
- An installed prefix uses the final program artifact name as its resource
  namespace: `<prefix>/bin/<name>` is paired with
  `<prefix>/share/<name>/`.

The source package name is not part of the runtime filesystem protocol.

## MVP scope

### Executable packages only

The `data_dir` option applies only to a package whose source target produces an
executable artifact. Declaring it on a library package is an error. A library
package does not contribute resources transitively to an importing executable.

This restriction prevents ambiguous merges and filename collisions. Supporting
library-owned resources would require a package namespace or another explicit
ownership model and is not an incremental extension of the single-argument
runtime API.

Multiple independent executable packages may each have their own resources.
The MVP does not infer a runtime or distribution relationship between those
executables.

### Source declaration

Given an executable package:

```text
app/
  moon.pkg
  main.mbt
  assets/
    templates/
      page.html
    schema.json
```

Its package manifest declares:

```text
pkgtype(kind: "executable")

options(
  data_dir: "assets",
)
```

`app/assets/` is then the source data directory. Its complete subtree is
application data. The MVP does not add include/exclude patterns, generated
resource declarations, or per-backend selection.

The `data_dir` path remains part of the runtime layout. In this example,
`@env.resource_path("assets/schema.json")` names the same file in a local build,
an installed prefix, or an application bundle. Distribution changes the
selected root, not the path below that root.

The path is slash-separated, normalized, and must remain inside the package.
Empty paths, absolute paths, Windows path prefixes or separators, and paths
that normalize outside the package are rejected. The declared path must exist
and be a directory.

The absence of `data_dir` means that the package has no application resources.
An undeclared `resources/` directory has no application-resource semantics.

### Commands

The initial command scope is:

- `moon build`, for executable artifacts it produces; and
- `moon run`, including `--build-only`, for the selected executable artifact;
- `moon test`, including `--build-only`, for test executables owned by an
  executable source package; and
- `moon bench`, for benchmark executables owned by an executable source
  package.

Tests and benchmarks are included because resource-dependent executable code
must be testable without a different lookup convention. A library package does
not acquire resource ownership merely because `moon test` or `moon bench`
produces a runnable test artifact for it.

Directly running an existing Wasm file does not create a development mapping.
It consumes whichever runtime layout already exists beside or around that
file.

`moon check` does not create a mapping because it does not produce a runnable
program artifact. Dry-run modes do not mutate the filesystem.

## Runtime API

The public API is a small wrapper in `moonbitlang/core/env`, alongside other
process-environment facts such as command-line arguments and the current
directory:

```moonbit
pub fn resource_path(name : String) -> String raise ResourceError

pub suberror ResourceError {
  InvalidResourceName(String)
  ResourceRootNotFound
  ResourceNotFound(String)
  ResourcePathNotRepresentable
  ResourceBackendUnsupported
} derive(@debug.Debug)
```

The wrapper has no required resource-root, package-name, distribution, or
component argument in the MVP.

The first delivery guarantees this API for Wasm and Wasm-GC programs executed
by moonrun. The function remains present for other core backends but raises
`ResourceBackendUnsupported` until a backend can provide an equivalent
program-artifact anchor and host filesystem path. This preserves one source API
without pretending that browser JavaScript or every native runtime already has
the required host integration.

Moonrun provides the private host primitive used by the Wasm wrapper. The
primitive receives or captures the path of the Wasm program passed to moonrun.
It must not anchor lookup to the `moonrun` executable. Both the wrapper and the
host primitive validate logical names so bypassing the wrapper cannot turn the
primitive into an unrestricted path join.

The implementation should remain concentrated behind the wrapper so a future
advanced API can accept an optional root or component context without changing
the default call. No such optional argument is exposed until a concrete use
case determines what it must represent.

`SourceLoc` is not used as implicit context. A call site identifies source
text, not the runtime component that owns a distributed artifact; the same
library function could also be called by several executables. If a future
component model needs disambiguation, it should supply an explicit optional
component or resolver context behind the default wrapper.

### Resource names

A resource name is a portable path relative to the selected application root.
It includes the configured `data_dir` component; the runtime does not need
source-package metadata to remove or reconstruct that component. A resource
name:

- it is non-empty and relative;
- `/` separates components on every platform;
- `\` is rejected rather than treated as a platform-dependent separator;
- absolute paths and platform path prefixes are rejected;
- empty, `.` and `..` components are rejected; and
- NUL is rejected.

Nested files and directories may be requested. The resolved path must exist.
The API returns an absolute host path suitable for the platform's filesystem
APIs. Whether the returned path is physically canonicalized is not part of the
MVP contract; development mappings must remain usable without exposing their
source target as a public guarantee.

### Errors

Lookup maps failures to `ResourceError` as follows:

- invalid logical names raise `InvalidResourceName`;
- absence of an applicable root raises `ResourceRootNotFound`;
- a missing name below the selected root raises `ResourceNotFound`;
- a host path that cannot be represented as a MoonBit `String` raises
  `ResourcePathNotRepresentable`; and
- a backend without a program-artifact resolver raises
  `ResourceBackendUnsupported`.

Returning an empty string or a path to a nonexistent candidate is not allowed.

## Resource-root selection

The program artifact means the runnable MoonBit output: a Wasm file for
moonrun, a JavaScript entry file for Node, or a native executable. It is not
necessarily the operating-system process executable.

For a file-based runtime such as moonrun, the artifact path is made absolute
when the runtime is initialized, before guest code can change the working
directory. This is a lexical absolute path: it is not canonicalized through a
symlink, because the invoked location determines whether an installed prefix or
bundle layout applies.

The runtime selects and stores one root for the lifetime of the runtime
instance, either eagerly or on the first request. It does not re-evaluate
provider precedence between calls. The resolver examines the program artifact
using the following ordered providers.

### 1. macOS application bundle

If the program artifact is contained in a macOS application bundle, the
bundle's resources directory is the candidate root. The MVP recognizes the
nearest ancestor shaped as:

```text
<name>.app/
  Contents/
    Info.plist
    Resources/
```

The artifact must be below that `Contents` directory, `Info.plist` must be a
file, and `Resources` must be a directory. Parsing the property list is not
required for resource-root selection.

The implementation must identify the bundle containing the program artifact.
It must not blindly use the main bundle of the current OS process, because a
system-installed moonrun executing a user Wasm file would otherwise select
moonrun's bundle.

For that reason the moonrun implementation uses the program path and bundle
layout directly rather than `CFBundleGetMainBundle`. A future native backend
may use platform bundle APIs only if it still identifies the bundle containing
the MoonBit program artifact.

The candidate is applicable only when the bundle and its resource directory
exist. Once selected, a missing requested resource is an error; lookup does not
fall back to an installed-prefix or sibling directory.

This MVP promise applies to the distribution entry executable. Resource
namespaces for future helper executables inside the same application bundle
belong to the future component model.

### 2. Installed prefix

If the program artifact is directly inside a directory named `bin`, let its
parent be `<prefix>` and derive `<name>` from the final artifact filename.
Remove exactly one known MoonBit runnable suffix: `.wasm`, `.js`, or `.exe`.
All other suffixes remain part of the name, and a suffixless filename is used
unchanged. For example, `editor.preview.wasm` maps to
`share/editor.preview/`.

`.wat` and `.rspfile` do not participate because they are not runtime program
artifacts. If removing a suffix would produce an empty name, this provider is
not applicable.

The candidate root is:

```text
<prefix>/share/<name>/
```

This candidate is applicable only when that directory exists. If it does not,
lookup continues to the sibling provider.

The name is the final program artifact name, not the package's fully qualified
name. A future distribution step may rename the artifact; it must materialize
the matching `share/<name>` directory at the same time. This keeps the runtime
protocol independent of source package metadata.

### 3. Sibling artifact directory

The final candidate is:

```text
<program-artifact-parent>/
```

This is the development layout created by `moon build` and `moon run`, and it
may also be used by manually assembled portable layouts.

The program artifact's parent must exist for this provider to apply. If it does
not, lookup reports that no resource root was found.

### Root selection is not file fallback

Candidate-root existence determines which provider applies. After a root is
selected, the requested name is resolved only under that root.

For example, if `<prefix>/share/tool/` exists but does not contain
`assets/schema.json`, a sibling `bin/assets/schema.json` must not satisfy the
request. Mixing roots would let incomplete or stale installed layouts
silently read development files.

## Build and run integration

After a successful build of an executable package with a declared data
directory, Moon reconciles this mapping:

```text
<artifact-parent>/<data_dir> -> <package-root>/<data_dir>
```

On Unix the destination is a symbolic link. On Windows it is an NTFS junction,
avoiding the privilege and developer-mode requirements that may apply to
directory symbolic links. A Windows filesystem without junction/reparse-point
support, or a resource directory on a remote share, produces a clear error;
the development command does not silently switch to copying.

The mapping is part of the generated development layout, not a compiler output
and not a resource copy. Changes below the declared data directory become
visible without rebuilding the executable.

The same source package may produce several applicable artifacts. If they share
an artifact parent they also share the same mapping; if they use different
parents, each parent receives a mapping. Two different executable packages must
not silently reconcile the same destination to different source directories.
The current target layout keeps their artifact parents distinct; a future
layout that violates that invariant must report a conflict.

### Reconciliation and lifecycle

While `data_dir` is declared, its artifact-side destination is reserved by
Moon as part of the target directory layout.

Reconciliation must be idempotent:

- an existing mapping to the expected source directory is left unchanged;
- an existing symbolic link or junction at the reserved destination may be
  replaced when its target is stale; and
- a real file or directory at the destination is never recursively removed;
  it causes a clear error.

A declared path that does not exist is a configuration error.

The implementation must use link-aware metadata so it does not follow a stale
or user-controlled destination while deciding what to replace.

Moon does not keep an inventory of development mappings. Removing or changing
`data_dir` may therefore leave an old mapping or an empty parent directory in
the disposable target directory. `moon clean` removes that stale build state.
In particular, changing between nested paths whose current and old
destinations overlap may require cleaning the target directory first.

For `moon run`, reconciliation occurs while Moon still owns the target
directory lock and before that lock is released for process execution. This
preserves the accepted `moon run` lifecycle: the lock is still released before
the user program starts, and no resource setup is delegated to the child.

`moon test` and `moon bench` perform the same reconciliation before launching
an applicable generated executable. Build-only modes leave the mapping ready
beside the reported artifact. A successful watch iteration reconciles mappings;
a failed build and a dry run do not mutate them.

The mapping may be implemented as command-level post-build reconciliation
rather than a compiler or n2 build action. That choice is not a public
contract. A future development staging tree may replace links without changing
the runtime API or root-selection rules.

## Responsibility boundaries

### Package discovery

Package discovery records the normalized `data_dir` declaration together with
the executable package root. Once it reads that declaration, it skips the
complete data-directory subtree during package scanning. A `moon.pkg`,
`moon.pkg.json`, or MoonBit-shaped file below `data_dir` remains application
data: it neither declares a nested package nor enters a package file set.
Command orchestration inspects `<package-root>/<data_dir>` on demand.

### MoonBuild and command orchestration

MoonBuild identifies executable artifacts and their owning packages. Command
orchestration owns development mapping because it has the selected source
package, final artifact path, target-directory lock, and command lifecycle.

The current `build`, `run`, `test`, and `bench` command pipelines use Rupes
Recta, so there is no selectable legacy build-engine path to mirror in this
MVP. If another engine becomes selectable again, these command-level semantics
require parity.

### moonrun

Moonrun owns Wasm-host path resolution and the host primitive consumed by the
standard library wrapper. The resolver is based on the guest Wasm program
artifact path.

Resolving and returning a resource path does not grant permission to read it.
When a restrictive moonrun policy is present, subsequent filesystem operations
must already be allowed by its `[fs].read` roots. The resolver does not
implicitly add the selected resource root to that list.

An implicit grant would be unsafe: a development application root contains a
symbolic link or junction at `<data_dir>`, and a manually assembled root can
also contain links. Treating either as an automatic capability could expand a
narrow policy to an unrelated host directory. A future resource API that opens
files itself may define a separate, capability-aware policy contract; the MVP
path-returning API does not.

### Standard library

The `moonbitlang/core/env` package owns the user-facing wrapper, portable
validation semantics, and `ResourceError`. Its public names do not expose
moonrun. Moonrun supplies the first working backend implementation; other
backends retain the API and report `ResourceBackendUnsupported`.

## Deliberate non-goals

The MVP does not design or implement:

- `moon dist`;
- archive, installer, Flatpak, macOS application, or other distribution
  generation;
- `moon serve`, watch/restart supervision, port allocation, readiness, or log
  aggregation;
- runtime dependencies between executable packages;
- building several backend artifacts as one application;
- transitive or library-owned resources;
- resource embedding into Wasm, JavaScript, or native binaries;
- localization or CFBundle-style filename selection;
- generated resources or manifest-controlled filtering; or
- mutable application state, configuration, caches, or user data.

The existing `moon bundle` command bundles standard-library compiler artifacts
and is unrelated to application-resource distribution.

## Extension seams

The MVP is intended to survive later application orchestration.

Internally, lookup may be represented as a simple resolver constructed from a
program artifact. The MVP derives every fact from its path. A future resolver
may additionally receive a component identity or an explicitly staged root,
while the one-argument public wrapper retains its default behavior.

Future application work should distinguish at least:

- build dependencies between produced artifacts;
- runtime dependencies on available executables;
- serve dependencies on running and ready processes; and
- distribution inclusion of components.

Those relationships do not belong in the package import graph. A future
application/component graph may feed both development orchestration and
distribution packaging. It may create a staged development prefix containing
`bin/` and `share/` or provide component-specific roots. Neither requires a
change to the MVP source declaration or default runtime wrapper.

## Validation plan

Implementation should proceed in test-driven slices.

### Runtime resolver

Unit tests should cover:

- invalid logical names and traversal attempts;
- making a relative program path absolute before guest execution;
- macOS bundle, installed-prefix, and sibling candidate order;
- derivation of names from `.exe`, `.wasm`, `.js`, and suffixless artifacts;
- retaining unknown suffixes and rejecting `.wat` and `.rspfile` as runtime
  artifact suffixes;
- selecting and caching a root once and refusing per-file fallback;
- missing roots and missing resources; and
- reporting an unrepresentable path for non-UTF-8 platform components.

### Development mapping

Filesystem tests should cover:

- creation and idempotent reuse of a symbolic link on Unix;
- creation and idempotent reuse of an NTFS junction on Windows;
- stale mapping replacement;
- rejection of missing, non-directory, or escaping `data_dir` values;
- refusal to replace a real destination directory; and
- multiple executable packages receiving independent mappings.

### End-to-end behavior

End-to-end tests should demonstrate:

- a Wasm executable reads the same resource through `moon run` and from its
  built artifact;
- changing a source resource is visible without recompiling the executable;
- `moon run --build-only` reports an artifact whose sibling mapping is ready;
- tests and benchmarks in an executable package receive the same mapping while
  library-package tests do not acquire resource ownership;
- a manually staged `bin/<name>` plus `share/<name>` layout resolves correctly;
- restrictive moonrun policy does not receive an implicit read grant; and
- a macOS bundle layout selects `Contents/Resources` without selecting
  moonrun's process bundle.

Test additions must preserve the existing `moon run` process-lifecycle
invariants.

## Resolved MVP questions

The decisions that previously blocked implementation are:

1. The public API is `@env.resource_path`, accepting a `String` and returning a
   `String` or raising `ResourceError`.
2. Moonrun-hosted Wasm and Wasm-GC are the first supported backends. Other
   backends expose the same wrapper and report `ResourceBackendUnsupported`.
3. `moon test` and `moon bench` reconcile mappings only for executable source
   packages.
4. The current command paths are Rupes Recta-only; there is no selectable
   legacy engine path to support.
5. Restrictive moonrun policy requires an explicit filesystem read grant.
6. A macOS bundle is identified from the program artifact's enclosing
   `.app/Contents` layout, not the current process's main bundle.
7. Installed-prefix naming strips one of `.wasm`, `.js`, or `.exe`; unknown
   suffixes remain part of the name.
8. Executable packages explicitly declare a package-relative source data
   directory with `options(data_dir: "...")`; there is no implicit
   `resources/` source convention.
9. `data_dir` is preserved below every selected application root. Runtime
   resource names include it, so development lookup remains anchored only to
   the program artifact and needs no package-manifest context.

No unresolved design question blocks implementation ticketing.

## Delivery plan

Delivery is split so each ticket has a focused failing test before its
implementation and can be reviewed independently.

### Milestone A: runtime contract

1. **Moonrun resolver and private host primitive**

   Add a pure Rust resolver with tests for logical-name validation, root
   precedence, stable root selection, artifact-name derivation, macOS bundle
   recognition, and error classification. Then expose it through a private
   moonrun host primitive whose program-path context is captured at runtime
   initialization. A MoonBit fixture may call the primitive directly so this
   ticket does not depend on a released core package.

2. **Core `@env.resource_path` wrapper**

   In `moonbitlang/core`, add `ResourceError`, portable validation, the Wasm
   host binding, and unsupported-backend implementations. Tests first cover
   every public error category and ensure the public wrapper has no moonrun
   names or context argument.

### Milestone B: development layout

3. **Cross-platform mapping reconciler**

   Add one command-layer primitive that creates, reuses, and replaces Unix
   symbolic links or Windows junctions without following a destination while
   classifying it. Filesystem tests first cover idempotence, stale mappings,
   real-directory conflicts, and missing sources.

4. **`moon build` and `moon run` integration**

   Derive mapping requests from successful Rupes Recta executable artifacts and
   owning packages. Reconcile them while the target lock is held, including
   watch and build-only paths, and preserve dry-run and process-lifecycle
   behavior. Integration tests first cover the sibling layout and live source
   updates.

The initial Milestone B slice is implemented for `moon build`, ordinary
`moon run`, and `moon run --build-only`. Watch-mode coverage and the broader
command parity in Milestone C remain follow-up work.

### Milestone C: command parity and layouts

5. **`moon test` and `moon bench` integration**

   Reuse the mapping reconciler for generated executables owned by executable
   packages. Tests first prove that executable-package tests receive resources
   and library-package tests do not gain resource ownership.

6. **Cross-layout end-to-end coverage**

   After a toolchain containing the core wrapper is available, test the same
   program through `moon run`, a built Wasm artifact, an installed
   `bin`/`share` prefix, and a macOS application-bundle fixture. Add a
   restrictive-policy case that requires an explicit read root.

The runtime and mapping milestones may be developed independently, but the
cross-layout test is the release gate for the user-facing feature.

## Reconsideration criteria

Reconsider the one-executable scope only when a concrete application requires
multiple produced executables to be built, served, or distributed together.
That design must address development layout and lifecycle as well as packaging;
adding transitive executable discovery only to a distribution command is not
sufficient.

Reconsider the path-returning API if a supported runtime cannot expose
application resources as host filesystem paths. Do not weaken missing-resource
errors or introduce implicit current-working-directory lookup merely to support
such a runtime.

## References

- [Apple: Placing content in a bundle](https://developer.apple.com/documentation/bundleresources/placing-content-in-a-bundle)
- [Filesystem Hierarchy Standard: `/usr/share`](https://specifications.freedesktop.org/fhs/latest/usrShare.html)
- [Microsoft: Hard links and junctions](https://learn.microsoft.com/en-us/windows/win32/fileio/hard-links-and-junctions)
