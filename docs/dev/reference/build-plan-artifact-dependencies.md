# Build plan artifact and product dependencies

Rupes Recta now uses a compiler-style split between planning, the action-level
plan consumed by lowering, and backend graph construction:

```text
BuildPlan       = planning IR: derivations, artifact requirements, provider selection
BuildActionPlan = action-level build plan: action ids, hydrated action data, artifacts and legacy outputs
LoweredAction   = concrete action: realized products, paths, and process command
n2::Build       = executor representation produced by the n2 adapter
```

The important invariant is that `build_lower` consumes `BuildActionPlan` only.
It does not import or match on planning internals such as `BuildPlanNode` or
`FileDependencyKind`. `ArtifactKey` is the Build Artifact identity carried
across this seam for migrated outputs, not a planning-internal selector.

## Backend configuration

After command adapters resolve the Target Backend and expand user intent to
concrete input nodes, Moon selects one `BackendConfig` value:

```rust
pub enum BackendConfig {
    Wasm { use_wat: bool, wasi_link: bool },
    WasmGc { use_wat: bool },
    Js,
    Native(NativeBackendMode),
    Llvm,
}
```

This value is the compile-wide source of truth passed through `CompileConfig`,
`BuildEnvironment`, and `BuildOptions`. Backend-specific options stay inside
their matching variant; lowering does not reconstruct a second backend enum
from a Target Backend plus optional native and Wasm flags. Code that only needs
the user-visible identity projects a `TargetBackend` directly from this value;
there is no separate build/run backend mirror.

The Native backend mode is currently compile-wide because the runtime product
is shared by the plan and C-stub products are shared per package. Selecting a
Native Payload Form per executable requires those shared products to be keyed
by the Native Toolchain and realization they were built for first.

## Planning IR

`BuildPlan` owns graph traversal, exact package artifact requirements, provider
registration, and action topology. The migrated package compilation artifacts
are:

```rust
pub enum ArtifactKey {
    CheckMi { package: PackageId, target_kind: TargetKind },
    BuildMi { package: PackageId, target_kind: TargetKind },
    CoreIr { package: PackageId, target_kind: TargetKind },
    VirtualContractMi { package: PackageId },
}
```

`CoreIr` names the compiler IR artifact written with the `.core` extension. It
is not related to the `moonbitlang/core` package.

Artifact identity does not include the physical output root or a provider
action ID. `CheckMi`, `BuildMi`, and `VirtualContractMi` are distinct because
they are not interchangeable compiler inputs, even though all three currently
use the `.mi` extension.

Builders record Artifact Requirements. The artifact rule schedules the unique
provider, and the provider registers its outputs when its derivation is fully
planned. After all providers have been planned, `BuildPlan` validates every
requirement. `BuildActionPlan` then resolves requirements directly to their
provider actions and logical products:

```rust
self.require_artifact(
    consumer,
    ArtifactKey::BuildMi { package, target_kind },
);
self.require_artifact(
    consumer,
    ArtifactKey::CoreIr { package, target_kind },
);
```

Normal downstream `BuildCore` derivations require both Build MI and Core IR so
a dependency implementation change that leaves its interface stable still
rebuilds the dependent package. Check derivations require Check MI only.
`LinkCore` and `Bundle` require Core IR, while `BuildDocs` requires Check MI.
Builders do not choose the `Check` or `BuildCore` derivation that satisfies
these requirements.

Virtual-contract compilation chooses the dependency MI artifact from the
invocation lifecycle:

```rust
match run_mode {
    RunMode::Check | RunMode::Prove => ArtifactKey::CheckMi { package, target_kind },
    RunMode::Build | RunMode::Run | RunMode::Test | RunMode::Bench | RunMode::Bundle => {
        ArtifactKey::BuildMi { package, target_kind }
    }
    RunMode::Format => unreachable!(),
}
```

There is no Check-to-Build coalescing. The two derivations provide different
artifacts and can never substitute for one another.

Every package MI and Core IR dependency remains an Artifact Requirement through
action planning; there is no compatibility projection to an action edge. Proof,
generated-test-info, C-stub, runtime, and other not-yet-migrated dependencies
continue to use `FileDependencyKind`.

Package-level prebuild has a different boundary. Each `BuildPlan` owns a
separate `PackagePrebuildPlan` containing complete custom prebuild, moonlex,
and moonyacc actions. `BuildActionPlan` combines it with the backend graph for
lowering without introducing logical edges between the two; matching physical
paths establish the dependencies in n2.

## Build Action Plan

`BuildPlan::build_action_plan()` creates the view consumed by backend lowering:

```rust
pub struct BuildActionId(usize);

pub enum BuildAction<'a> {
    Check { target: BuildTarget, info: &'a BuildTargetInfo },
    BuildCore { target: BuildTarget, info: &'a BuildTargetInfo },
    LinkCore { target: BuildTarget, info: &'a LinkCoreInfo, make_executable_info: Option<&'a MakeExecutableInfo> },
    MakeExecutable { target: BuildTarget, info: Option<&'a MakeExecutableInfo> },
    GenerateDsym { target: BuildTarget, dsymutil: &'a Path },
    // other action variants carry the same hydrated planning metadata
}
```

`MakeExecutableInfo` is present only for native executable work. For non-native
backends, `MakeExecutable` remains a final-artifact alias over `LinkCore` and is
a no-op in backend lowering.

Action outputs are exposed through `BuildProduct`. It is an action-level
envelope, not a second Build Artifact identity:

```rust
pub enum BuildProduct {
    Artifact(ArtifactKey),
    GeneratedTestDriver { target: BuildTarget },
    CStubObject { package: PackageId, index: u32 },
    Executable { target: BuildTarget },
    DsymBundle { target: BuildTarget },
    RuntimeObject { index: u32 },
    RuntimeLib,
    PrebuildOutputPath { path: PathBuf },
    // other logical outputs
}
```

Package compilation outputs use `BuildProduct::Artifact` and retain the exact
`CheckMi`, `BuildMi`, `CoreIr`, or `VirtualContractMi` key selected during
planning. In particular, Check MI and Build MI are never collapsed into a
generic interface product and then recovered from their provider action. The
remaining variants are outputs whose dependencies have not yet migrated from
action-coupled file selectors.

`PrebuildOutputPath` is the explicit exception: prebuild outputs are resolved
when the prebuild command is planned, so the product carries the already
resolved path instead of asking `ArtifactPathResolver` to reconstruct it from
package metadata.

Outputs for an action are just products:

```rust
pub fn output_products(&self, id: BuildActionId) -> Vec<BuildProduct> {
    self.output_products_for_node(self.node(id))
}
```

Artifact Requirements and remaining file selectors become `(dependency action,
product)` pairs. The dependency action is context for path resolution; it is
not part of Build Artifact identity:

```rust
pub fn dependency_products(&self, id: BuildActionId) -> Vec<(BuildActionId, BuildProduct)> {
    let node = self.node(id);
    let file_dependencies = self.plan.file_dependencies(node).flat_map(/* ... */);
    let artifact_dependencies = self.plan.artifact_dependencies(node).map(/* ... */);
    file_dependencies.chain(artifact_dependencies).collect()
}
```

If one provider supplies multiple required artifacts, each
`BuildProduct::Artifact` retains its separate key while action topology contains
the provider only once. This lets Build MI and Core IR share a compiler
invocation without collapsing their identities.

For example, an archive/link C-stub action no longer scans raw build-plan
edges in `build_lower`. Its object inputs are exposed by `BuildActionPlan` as
`(object_action, BuildProduct::CStubObject { ... })` dependencies.
The runtime archive follows the same contract with `RuntimeObject`
dependencies and a `RuntimeLib` output. Optional prebuilt SIMDUTF objects in
release builds are selected during planning and become external file inputs of
the archive action because the build plan does not produce them. For archivers
that update an existing archive, planning also computes the ordered member-list
fingerprint once for runtime and C-stub archives. Product realization uses it
in the static archive path, so consumers do not recalculate the fingerprint.
Archivers that recreate the archive from the complete input list leave this
fingerprint unset and retain stable output paths.

## Action Lowering and n2 Adaptation

`build_lower` matches on `BuildAction` and resolves `BuildProduct` paths through
`ArtifactPathResolver`. Migrated products are resolved directly from their
`ArtifactKey`; the provider action is checked as provider context rather than
used to reconstruct artifact identity:

```rust
let cmd = match self.plan.action(id) {
    BuildAction::Check { target, info } => self.lower_check(&products, target, info),
    BuildAction::BuildCore { target, info } => self.lower_build_mbt(&products, target, info),
    BuildAction::ArchiveOrLinkCStubs { package, info } => {
        self.lower_archive_or_link_c_stubs(&products, package, info)
    }
    // ...
};
```

`ActionProducts` is built per action. Outputs are resolved with the current
action as context; dependencies are resolved with the dependency action from the
edge tuple:

```rust
let outputs = plan
    .output_products(action)
    .into_iter()
    .map(|product| realize(action, product));

let dependencies = plan
    .dependency_products(action)
    .into_iter()
    .map(|(dependency_action, product)| realize(dependency_action, product));
```

The command, realized dependency products, external file inputs, outputs, and
execution metadata are assembled into one `LoweredAction`. Ordinary builds move
each action directly into the n2 adapter as soon as it is lowered; they do not
retain a second complete graph in memory. The adapter alone registers n2 files,
constructs `n2::Build`, and reports n2 graph errors.

Every lowered command retains structured argv. Response files change only the
transport recorded alongside that argv. At the common lowering boundary, the
first argument's concrete executable path becomes an external file input.
External inputs are sorted and deduplicated before the action reaches n2, and
the original argv remains authoritative when transport switches to a response
file.

External inputs may also describe semantic filesystem observations that n2
cannot represent as ordinary file edges. Compiler actions using `-std-path`
carry the selected standard-library interface bundle as a recursive `.mi`
input. The n2 adapter intentionally omits that semantic directory from its file
list, while action identity digests it once per calculation. Native compilation
enumerates the selected Moon toolchain include tree once per lowering context,
then attaches those headers as ordinary file inputs. Lowering also attaches
Moon-owned runtime objects and archives when command construction selected
their exact paths.

Lowering marks actions with broader, not-yet-modeled observations as
cache-ineligible. This covers proof execution, documentation generation,
arbitrary prebuild shell commands, and actions that execute unstructured custom
compiler or linker flags. Lowering does not infer file inputs by parsing those
flags.

Lowering always exposes concrete paths. The cache identity layer hashes those
paths and all command text exactly; it does not replace path roots or interpret
tool-specific argument syntax. This keeps cache policy out of lowering and
ensures that moving a source, toolchain, work directory, or output produces a
conservative miss.

When one build result requires two processes, planning represents them as
separate actions instead of joining rendered command strings with shell
operators. For example, macOS debug builds use `MakeExecutable` for the linker
invocation and a dependent `GenerateDsym` action for `dsymutil`. The latter
consumes the executable product and produces the `.dSYM` bundle.

This keeps responsibilities separate:

- `BuildPlan` owns Artifact Requirements, provider registration, and combined action topology.
- `BuildActionPlan` owns the normalized action/artifact interface between phases.
- `ArtifactPathResolver` owns artifact and legacy output path resolution.
- action lowering owns concrete products, external inputs, and commands.
- the n2 adapter owns n2 graph construction.

## Standalone script boundary

Standalone `.mbt` and `.mbtx` execution starts from one complete `BuildPlan`.
Package dependencies use the same Artifact Requirements as ordinary project
compilation. After the plan is normalized once, an action-level projection
retains dependency work as `LoweredAction` values and lowers script work into
an n2 graph. Moon passes the dependency actions through the current n2 adapter
before execution, producing the same two disjoint n2 graphs as before.

For `moon run` standalone inputs, registry package acquisition is separate from
that build projection. Persistent standalone `.mbt` and `.mbtx` files, inline
`-e` programs, and stdin programs passed as `-` resolve registry modules to
immutable entries in the global dependency source cache. Entries are addressed
by module and version and published atomically. Each entry records the SHA-256
of the registry ZIP archive that produced it. Reuse compares that metadata with
the current registry index and validates the `moon.mod` or `moon.mod.json`
manifest; a checksum change is an error that requires an explicit dependency
cache clean rather than a replacement or a second path. Registry acquisition
uses one selected checksum to verify and extract one open archive handle.
`MOON_DEP_CACHE=off` retains the previous
project-local or temporary `.mooncakes` preparation. Single-file check and test
commands do not use this global source path.

All actions owned by packages other than the synthesized script package seed
the dependency projection. Following their producer edges to a fixed point
also includes package-less shared prerequisites such as `BuildRuntimeLib` and
its `BuildRuntimeObject` dependencies.
Package-less actions needed only by the script stay in the script projection.
The projection is invalid if dependency preparation reaches a script-owned
producer.

The script projection keeps the original product edges. If a producer belongs
to the dependency projection, normal product realization still resolves its
concrete `.mooncakes` paths using that producer's action context. Those paths
become ordinary n2 inputs with no producer in the script graph; no external
product or path vocabulary is needed.

Execution runs the dependency graph first using
`standalone-dependencies.moon_db`, then runs the script graph using the existing
mode database. There is no file-existence scan between the phases: n2 currently
decides whether dependency actions are current.

The dependency n2 graph is an adapter for the current preparation mechanism.
A future action-to-output implementation can replace that first stage, then
feed the materialized outputs into a narrower script plan without changing the
dependency product contract.

For `.mbt` and `.mbtx` files built from persistent paths, the dependency n2
database remains available to later invocations. `moon run -e` and
`moon run -` also use the split graphs, but their synthesized temporary projects
are removed after each invocation. They reuse globally prepared registry
sources but do not yet reuse the dependency n2 database or compiled dependency
artifacts across invocations.

Ordinary project and workspace commands continue to produce and execute one
plan and one n2 graph.

## Compatibility

`LoweringResult` returns root action artifacts as `(BuildActionId, paths)` pairs
in input action order. The compile layer re-keys those artifacts back to
`BuildPlanNode` for the existing public `CompileOutput` shape. That keeps
compatibility above lowering while proving that backend lowering no longer sees
planning internals.

For a native macOS debug target, the existing `MakeExecutable` root reports the
executable first and its follow-up `.dSYM` bundle second. Requesting both paths
causes n2 to run the dependent `GenerateDsym` action without changing the
caller-visible executable artifact position.

## Checks

The boundary can be checked with:

```sh
rg -n '\bBuildPlan\b|BuildPlanNode|FileDependencyKind|PlanArtifact' \
  crates/moonbuild-rupes-recta/src/build_lower
```

There should be no meaningful matches.
