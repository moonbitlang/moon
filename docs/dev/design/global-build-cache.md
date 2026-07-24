# Global build state and cache design

## Status

This document records the intended direction for global dependency and build
caches. The cache-root contract, explicit cleaning, and globally prepared
registry dependency sources for `moon run -e`, `moon run -`, and
`moon run <file>.mbtx` are implemented today. Those commands also reuse one
complete registry dependency build graph through the global build cache.
Other commands retain project-local `.mooncakes` and do not read or publish
global compiler artifacts.

## Problem

Standalone commands such as `moon run -e` and `moon run mbtx` repeatedly
download, unpack, and compile the same dependencies. Reusing one mutable
`_build` directory is not a safe solution: concurrent or differently
configured invocations can overwrite each other's state. Keeping every
invocation entirely private avoids conflicts but wastes work, especially when
most of the package graph is unchanged.

Moon therefore needs two different global facilities:

- a dependency-source cache for acquired and prepared package sources; and
- a build cache for reusable compiler outputs such as `.mi` and `.core`.

Mutable work directories remain private to an invocation. This separation is
the central design constraint: globally share immutable inputs and validated
outputs, never a live `_build` tree.

## Decision summary

| State | Owner and lifetime | Shared? |
| --- | --- | --- |
| Downloaded or prepared dependency sources | Dependency cache | Yes, immutable after publication |
| Compiler artifacts (`.mi`, `.core`, and related outputs) | Build cache | Yes, after complete validation |
| Compiler temporaries and generated working files | Invocation `_build` | No |
| Final user-requested output | Command or project | No implicit global path |

Source acquisition and compiler outputs have different identities, lifetimes,
and cleanup rules, so they use separate caches. An artifact is reusable only
when all compiler-observable inputs have the same identity. Directory names
are for storage organization, not correctness.

### Prepared dependency source identity

A prepared registry source is identified by its resolved `module@version`.
The registry checksum is immutable metadata for that identity, not another
component that permits multiple source variants. On first use, Moon verifies
the downloaded archive against the registry checksum, extracts into staging,
renames the complete entry while holding the module lock, and marks the entry,
including its checksum metadata and source tree, read-only before releasing
the lock. Concurrent publishers of the same identity must be harmless.

On a cache hit, Moon compares the current registry checksum with the checksum
recorded at publication after validating the entry's expected shape and
read-only state. A mismatch is an error: published registry versions are
immutable, so Moon must not replace the source or create a second checksum-keyed
variant for the same `module@version`.

The first rollout is limited to standalone `.mbtx` execution: `moon run -e`,
`moon run -`, and `moon run <file>.mbtx` resolve registry dependencies from
the shared cache. Other project and single-file commands retain the existing
project-local `.mooncakes` installation behavior. This keeps the initial
migration narrow while preserving the cache format and correctness rules
needed by later command migrations.

### Standalone dependency build identity

The first compiler-artifact rollout uses the complete registry dependency
subgraph of one standalone script as its cache unit. It does not yet reuse a
partially overlapping set of dependency packages. Rupes Recta identifies the
registry-source `BuildCore` actions during normal lowering; it does not create
a second build model for the cache.

The dependency graph ID is a SHA-256 digest over the selected packages,
canonical compiler arguments, exact resolved module versions, logical
dependency-product edges, and the contents of compiler, standard-library,
source, and configuration inputs. Physical checkout, target, and cache paths
are replaced by logical labels, so equivalent standalone scripts in different
directories can share the graph. File digests are memoized within an
invocation, so the compiler binary is hashed only once.

On a miss, one n2 graph builds all dependency products with its normal
parallelism. Moon publishes the complete `.mi`/`.core` set while holding the
graph lock, then runs the script's project-local graph. On a hit, Moon
validates every recorded artifact and restores it into the invocation's
private target directory; the local n2 graph then treats those files as
external inputs. The n2 database, `packages.json`, and mutable build workspace
remain invocation-local and are never shared through the cache.

A later implementation may partition this graph into package or action
records to reuse partial overlap. That changes the reuse granularity, not the
current graph identity or publication invariants.

## Implemented public seam

Two environment variables select the cache roots:

| Variable | Unset | `off` | Absolute path |
| --- | --- | --- | --- |
| `MOON_DEP_CACHE` | `$MOON_HOME/cache/deps` | Disable dependency caching | Use that dependency-cache root |
| `MOON_BUILD_CACHE` | `$MOON_HOME/cache/build` | Disable build caching | Use that build-cache root |

A relative path is rejected when a command selects the corresponding cache.
`off` makes the three standalone `.mbtx` run forms use their file-local
`.mooncakes` flow instead; disabling the cache does not disable dependency
resolution. Commands that have not migrated do not consult `MOON_DEP_CACHE`
during dependency sync. For the three migrated forms,
`MOON_BUILD_CACHE=off` disables compiler-artifact reuse. Other commands do not
consult the build cache.

### Cleaning

`moon clean` keeps its existing meaning and removes the project's local build
directory. Global state is explicit:

```text
moon clean --dep-cache
moon clean --build-cache
moon clean --dep-cache --build-cache
```

When either cache flag is present, only the selected global cache roots are
cleaned; the local `_build` is left alone. A disabled or missing root is a
successful no-op, so these commands work outside a project.

Deleting a user-configurable absolute path is dangerous. Moon therefore
removes a non-empty root only when it contains Moon's matching ownership
marker. Empty roots may be removed, and symlinked or unrecognized roots are
refused. The marker is lifecycle safety metadata, not a promise about the
future data layout.

## Why `module@version` is not an artifact key

A dependency source can be identified by a resolved module version, but its
compiled interface cannot. Moon's version selection resolves one graph for the
whole invocation. If that resolution selects a different version of an
upstream package, a downstream package's `.mi` or `.core` can change even when
the downstream source and version do not.

For example, suppose `D` depends on `B` and `C`, while `B` requests `A@v1` and
`C` requests `A@v2`. Resolution selects one version of `A` for the invocation.
If `B` is compiled against the selected `A@v2`, its artifact cannot be assumed
equivalent to an earlier `B` artifact compiled against `A@v1`.

Moon does not guarantee that `.mi` or `.core` is invariant under such an
upstream change. The cache must conservatively identify a complete compilation
action, not merely a package version.

## Artifact identity

Artifact identity has three concepts:

- **Dependency graph ID:** the first implementation's digest of all inputs to
  the standalone script's registry dependency compilation.
- **Action ID:** a future finer-grained digest of everything that can affect
  one compilation action.
- **Output ID:** a digest of the complete published result.
- **Cache record:** a mapping from the graph or action ID to its output
  metadata.

An artifact ID includes, in a deterministic encoding:

- package source contents and relevant generated inputs;
- compiler and tool identities;
- target and all compiler options that affect emitted artifacts;
- the exact resolved dependency graph visible to the action;
- identities of imported dependency interfaces; and
- environment values only when the compiler action actually observes them.

This list is semantic, not a fixed serialization format. The first artifact
cache implementation begins from compiler actions already produced by Rupes
Recta and combines all registry dependency actions into one conservative graph
identity. Later measurements can justify narrowing the key or partitioning the
graph into independently reusable actions.

All outputs of the dependency graph are treated as a unit. Moon publishes into
staging and renames the complete entry atomically where the platform permits.
A reader sees either a complete validated result or a miss. Concurrent writers
of the same graph are serialized by its lock.

The command that requested compilation is not automatically part of the key.
For dependency packages, `build`, `run`, and `test` may share artifacts when
they produce the same compiler action. They diverge only when their actual
inputs or requested outputs differ.

## Targets, cross compilation, and build constraints

`.mi` and `.core` are target-dependent. Target information therefore belongs
in the action ID from the beginning. It should not be represented only by a
new directory layer, because future configuration will include more than one
axis and some axes do not apply to every backend.

Moon does not yet need to invent an OS and architecture for JavaScript, Wasm,
or WasmGC. A future target descriptor can encode only applicable facts, for
example backend plus optional operating system, architecture, ABI, and runtime
capabilities. Native cross-checking can then select explicit OS and
architecture values without changing the cache's correctness model.

Conditional source selection should produce one explicit build configuration
for each invocation. Dependency-requested feature unification is not part of
this design. A future build-tag or `cfg` design should:

- resolve to one explicit build configuration for an invocation;
- participate in source selection before action IDs are computed;
- include the resulting compiler-observable configuration in action IDs; and
- avoid exposing cache paths as the user interface for selecting a target.

This leaves syntax and policy open while fixing the invariant that different
effective programs cannot share an action ID.

## Standalone execution

The desired standalone flow is:

1. Resolve the package graph.
2. Reuse or prepare immutable dependency sources.
3. Create a private work directory for the invocation.
4. Reuse the matching complete dependency graph.
5. On a miss, compile the dependency graph privately and publish its complete
   reusable result.
6. Link or run from private state, then remove it when appropriate.

The script's own rapidly changing compilation may often miss, but its stable
dependencies can still be hits. This is the main opportunity for faster script
startup.

`post-add` hooks will not run in globally shared prepared sources. They make
source state mutable and can have effects that are not captured by an artifact
key. The shared-source flow rejects a dependency that requires `post-add`,
rather than execute the hook, silently skip it, or fall back to a mutable local
installation. A sandboxed, explicitly keyed hook model would require a
separate design.

Top-level module prebuild configuration remains a build-time operation. It runs
for each invocation against the prepared read-only source, and its structured
output affects that invocation's build plan rather than the dependency-source
cache. Package-level prebuild commands and source generators are skipped for
registry dependencies; published archives must already contain their generated
outputs.

`__moonbin__` belongs in the invocation's mutable work directory, initially
under `_build`. If its producer later becomes cacheable, its outputs may be
published like other action results, but a cache location must not become the
command-visible executable path.

Binary-dependency build isolation is independent of this first shared-source
rollout. Registry bin-deps are copied to target-owned temporary work before
building, as described in the architecture reference; local bin-deps retain
their existing behavior.

Moving `.mooncakes` wholesale into `_build` is not a prerequisite. Acquisition,
prepared immutable sources, mutable work, and final outputs have different
ownership and cleanup rules and should remain separate concepts.

## Delivery stages

Each stage should be useful and reviewable without requiring the next:

1. **Root contract and lifecycle (implemented):** environment selection,
   disabled semantics, safe explicit cleaning, and no internal data layout.
2. **Prepared dependency sources (partially implemented):** immutable
   publication for the three standalone `.mbtx` run forms, no shared
   `post-add`, and file-local fallback when caching is off. Other commands can
   migrate separately after this path is established.
3. **Private binary-dependency work (implemented):** stop sharing mutable
   `_build` trees between binary-dependency invocations; place `__moonbin__`
   there.
4. **Standalone dependency graph cache (implemented):** define canonical
   compiler inputs at the Rupes Recta boundary and publish or restore the
   complete registry dependency `.mi`/`.core` set.
5. **Finer-grained artifact cache:** partition reusable package or action
   results when partial graph overlap justifies the added model and storage
   complexity.
6. **Build constraints and cross compilation:** extend the target descriptor
   and action identity without changing storage-path semantics.
7. **Operations:** add recency tracking, pruning, diagnostics, and format
   migration only after real cache data exists.

Each implementation should choose one stage, write a failing end-to-end test
first, and avoid introducing speculative directory structure for later stages.

## References

- [Go module cache](https://go.dev/ref/mod#module-cache): separation of
  downloaded module state and compiled outputs.
- [Go build IDs](https://go.dev/src/cmd/go/internal/work/buildid.go#L26):
  action and output identities.
- [Go action hashing](https://go.dev/src/cmd/go/internal/work/exec.go#L260):
  inputs used to identify a compilation action.
- [Go build constraints](https://pkg.go.dev/cmd/go#hdr-Build_constraints):
  per-invocation source selection.
- [Cargo features](https://doc.rust-lang.org/cargo/reference/features.html):
  dependency-requested feature selection and unification.
- [Cargo fingerprints](https://doc.rust-lang.org/nightly/nightly-rustc/cargo/core/compiler/fingerprint/index.html):
  dependency and compiler-input invalidation.
