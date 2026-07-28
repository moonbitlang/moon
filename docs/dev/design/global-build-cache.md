# Global build state and cache design

## Status

This document records the intended direction for global dependency and build
caches. The cache-root configuration and cleaning contract described below are
implemented. Builds do not yet read from or write to either configured global
cache.

Single-file dependency preparation additionally uses a local action-to-output
store under `_build/.mooncakes/.build-cache`. That store exercises the action
identity and publication model, but it is path-sensitive, project-local, and
unaffected by `MOON_BUILD_CACHE`.

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

## Implemented public seam

Two environment variables select the cache roots:

| Variable | Unset | `off` | Absolute path |
| --- | --- | --- | --- |
| `MOON_DEP_CACHE` | `$MOON_HOME/cache/deps` | Disable dependency caching | Use that dependency-cache root |
| `MOON_BUILD_CACHE` | `$MOON_HOME/cache/build` | Disable build caching | Use that build-cache root |

A relative path is rejected. `off` means that a future build must use
invocation-private temporary state instead of the corresponding global cache.
For a disabled dependency cache, dependencies still have to be downloaded and
prepared somewhere private; disabling the cache must not disable dependency
resolution.

The environment variables currently configure and clean roots only. Their
presence does not yet change build execution.

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

- **Action ID:** a digest of everything that can affect the compilation.
- **Output ID:** a digest of the complete published result.
- **Action record:** a small mapping from an action ID to its output ID and
  output metadata.

The local single-file store identifies an action from its `LoweredAction`. Its
deterministic encoding includes:

- structured arguments rather than an n2-rendered command string;
- working directory and normalized environment;
- response-file path and content;
- external input kind, path, and BLAKE3 content digest;
- recursively identified dependencies, represented by producer action digest,
  logical product, and concrete paths; and
- concrete output paths.

Compiler and archiver executables are resolved to concrete files before
lowering and are external inputs. Standard-library `-std-path` input is modeled
as the recursively loaded `.mi` tree rather than hashing unrelated bundle
artifacts. Producer `BuildActionId` values are lookup handles for one lowering
only and never enter the persistent key.

The identity remains semantic rather than a serialization of every executor
field. Diagnostic descriptions, file locations, n2 dirtiness policy, and other
properties that do not affect emitted bytes are excluded. Verbatim shell
commands are currently uncacheable; structured commands are the auditable
cache boundary.

All outputs of one action are treated as a unit. Moon publishes a complete
content-identified object first and its action record last. Restore validates
the manifest and every file before materialization. A malformed record,
damaged manifest, missing file, partial object, or unknown format version is a
miss.

Publication uses staging and atomic rename where the platform permits. If
another writer wins the object race, the loser validates the winner and
succeeds. This property is local to immutable object and action-record
publication. A complete build sharing one `_build` still depends on Moon's
target-directory lock; the local store does not make arbitrary concurrent
writes elsewhere in the build tree safe.

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

## Single-file execution

The implemented local flow is:

1. Resolve the package graph.
2. Lower dependency packages to retained `LoweredAction` values.
3. Restore complete matching dependency outputs from the local store.
4. Convert only misses through the controlled n2 adapter.
5. Revalidate external inputs and publish complete reusable results.
6. Build and run the script graph against the materialized paths.

The script's own rapidly changing compilation may often miss, but its stable
dependencies can still be hits. This is the main opportunity for faster script
startup.

`post-add` hooks will not run in globally shared prepared sources. They make
source state mutable and can have effects that are not captured by an artifact
key. The initial shared-source flow should reject a dependency that requires
`post-add`, rather than execute the hook or silently skip it. A sandboxed,
explicitly keyed hook model would require a separate design.

`__moonbin__` belongs in the invocation's mutable work directory, initially
under `_build`. If its producer later becomes cacheable, its outputs may be
published like other action results, but a cache location must not become the
command-visible executable path.

Moving `.mooncakes` wholesale into `_build` is not a prerequisite. Acquisition,
prepared immutable sources, mutable work, and final outputs have different
ownership and cleanup rules and should remain separate concepts.

## Delivery stages

Each stage should be useful and reviewable without requiring the next:

1. **Root contract and lifecycle (implemented):** environment selection,
   disabled semantics, safe explicit cleaning, and no internal data layout.
2. **Prepared dependency sources:** immutable publication in the dependency
   cache, no shared `post-add`, and private fallback when caching is off.
3. **Private single-file work:** stop sharing mutable `_build` trees between
   single-file invocations; place `__moonbin__` there.
4. **Local action model (implemented for single-file dependencies):** canonical
   lowered action inputs, recursive producer identities, and selected-miss
   execution.
5. **Local artifact store (implemented for single-file dependencies):** publish
   and restore complete result sets with concurrency and corruption handling.
6. **Build constraints and cross compilation:** extend the target descriptor
   and action identity without changing storage-path semantics.
7. **Operations:** add recency tracking, pruning, diagnostics, and format
   migration only after real cache data exists.

Global reuse still requires prepared immutable sources, private build work, a
global storage policy, and operations. The local format is evidence for those
steps, not a compatibility promise for the global layout.

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
