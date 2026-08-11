# Build plan artifact dependencies

Rupes Recta uses one logical artifact model from planning through lowering:

```text
BuildPlan       = requested actions + artifact providers + artifact requirements
BuildActionPlan = stable action IDs + hydrated action data + artifact view
LoweredAction   = realized artifact paths + external inputs + process command
n2::Build       = executor representation produced by the n2 adapter
```

The key invariant is that an action depends on artifacts, not on another
action's position or an output selector attached to an action edge. Provider
selection is a planning concern. Lowering receives the selected provider only
as the context needed to realize the artifact's physical path.

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
`BuildEnvironment`, and `BuildOptions`. An `ArtifactKey` does not repeat the
backend, optimization profile, or run mode because one `BuildPlan` is scoped
to one such configuration. If one plan later contains multiple configurations,
that scope must become an explicit part of artifact identity before providers
can be shared safely.

## Build Artifact identity

`ArtifactKey` names a logical result independently of its provider action and
physical output root. The complete enum covers package compilation, proof,
test generation, linking, native support, documentation, and prebuild outputs.
Representative variants are:

```rust
pub enum ArtifactKey {
    CheckMi { package: PackageId, target_kind: TargetKind },
    BuildMi { package: PackageId, target_kind: TargetKind },
    CoreIr { package: PackageId, target_kind: TargetKind },
    VirtualContractMi { package: PackageId },

    ProofMi { package: PackageId, target_kind: TargetKind },
    ProofWhyml { package: PackageId, target_kind: TargetKind },
    ProofReport { package: PackageId, target_kind: TargetKind },

    CStubObject { package: PackageId, source: PathBuf },
    CStubLibrary { package: PackageId },
    RuntimeObject { source: PathBuf },
    RuntimeLibrary,
    GeneratedTestDriver { package: PackageId, target_kind: TargetKind },
    LinkedCore { package: PackageId, target_kind: TargetKind },
    Executable { package: PackageId, target_kind: TargetKind },

    PrebuildOutput { package: PackageId, path: PathBuf },
    // other logical artifacts
}
```

`CoreIr` names the compiler IR artifact written with the `.core` extension. It
is unrelated to the `moonbitlang/core` package.

Artifact identity never contains a `BuildActionId` or `_build` root.
`CheckMi`, `BuildMi`, and `VirtualContractMi` are distinct because they are not
interchangeable compiler inputs even though they all currently use `.mi`.
`EmitProof` and `Prove` are alternative providers of the same `ProofMi` and
`ProofWhyml` artifacts; only `Prove` additionally provides `ProofReport`.
Provider selection belongs to the invocation lifecycle, not artifact identity.

Static archives and TCC-run shared libraries are two physical realizations of
the same logical C-stub or runtime library artifact. Lowering selects the
realization from `BackendConfig`, just as it selects `.wasm` or `.wat` for a
`LinkedCore` or `Executable` artifact. `RuntimeObject` exists only on the
static-archive path; the TCC shared-runtime action consumes runtime sources
directly.

`Executable { package, target_kind }` includes test executables: source,
inline-test, whitebox-test, and blackbox-test targets have different
`TargetKind` values. Backend and profile remain configuration scope rather than
being repeated in every package artifact.

Package file artifacts use normalized declaration paths instead of list
indices. C-stub paths are relative to the package root. Supported prebuild
outputs follow the same rule; an explicitly absolute declaration outside the
package remains absolute because it has no package-relative identity. Runtime
object keys use their stable path within the toolchain library layout, such as
`runtime/foo.c`, rather than the installed toolchain root.

Only outputs that are explicitly selected, consumed, or exposed as roots need
artifact keys. Incidental compiler side files such as source maps or
declaration files remain part of their producing action's physical behavior
until Moon can independently request or suppress them.

## Planning IR

`BuildPlan` stores planned actions in stable insertion order and one artifact
registry:

```rust
pub struct BuildPlan {
    actions: IndexSet<BuildPlanNode>,
    artifacts: ArtifactPlan,
    // hydrated planning metadata
}

struct ArtifactPlan {
    providers: HashMap<ArtifactKey, BuildPlanNode>,
    artifacts_by_provider: HashMap<BuildPlanNode, IndexSet<ArtifactKey>>,
    requirements_by_consumer: HashMap<BuildPlanNode, IndexSet<ArtifactKey>>,
}
```

There is no weighted action-edge graph and no `FileDependencyKind`. A provider
may expose multiple artifacts, but each artifact has at most one provider in a
plan. At the end of planning, validation requires every Artifact Requirement
to have a provider.

Builders name only what they consume:

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

The central artifact rule maps each key to its provider action and schedules
that action. Builders do not encode `Check`, `BuildCore`, or another provider
in a dependency edge.

Normal downstream `BuildCore` actions require both Build MI and Core IR, so an
implementation change that leaves the interface stable still rebuilds the
dependent package. Check actions require Check MI only. `LinkCore` and
`Bundle` require Core IR, while `BuildDocs` requires Check MI.

Virtual-contract compilation chooses dependency MI artifacts according to the
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

There is no Check-to-Build coalescing. Check MI and Build MI have different
providers and cannot substitute for one another.

## Package prebuild actions

Each backend-specific `BuildPlan` owns a `PackagePrebuildPlan` containing
complete custom prebuild, moonlex, and moonyacc actions. This is an action
storage boundary, not a second dependency model.

Every generated file is registered as `PrebuildOutput { package, path }` in the
same `ArtifactPlan` used by backend actions. A prebuild action consuming an
earlier generated file records an Artifact Requirement, so custom-to-moonlex
and custom-to-moonyacc pipelines are explicit before n2 adaptation. Backend
actions likewise require the generated `.mbt`, `.mbt.md`, `.mbtp`, or `.mbti`
artifacts selected into their final file projection.

Unconsumed package prebuild actions remain planned under the existing
semantics. Their outputs become n2 start nodes because no later action consumes
them.

The prebuild action retains resolved execution paths and cwd. The artifact key
retains declaration identity. `ArtifactPathResolver` checks that the two agree
when it realizes the action's output.

## Build Action Plan

`BuildPlan::build_action_plan()` assigns plan-local `BuildActionId` values and
hydrates `BuildAction` data from planning metadata. It does not translate
between two dependency vocabularies.

```rust
pub fn output_artifacts(&self, id: BuildActionId) -> Vec<ArtifactKey> {
    self.plan.provided_artifacts(self.node(id)).collect()
}

pub fn dependency_artifacts(
    &self,
    id: BuildActionId,
) -> Vec<(BuildActionId, ArtifactKey)> {
    self.plan
        .artifact_dependencies(self.node(id))
        .map(|(provider, artifact)| (self.id_for_node(provider), artifact))
        .collect()
}
```

One provider can appear once in action topology while contributing several
separate artifacts. For example, one `BuildCore` action can provide both
`BuildMi` and `CoreIr`; consumers retain the exact identity of each required
artifact.

`BuildActionId` is only a plan-local address used during lowering and
diagnostics. It is not artifact identity and is not intended to become a
content or cache key.

## Action lowering and n2 adaptation

`build_lower` matches on `BuildAction` and resolves every `ArtifactKey` through
`ArtifactPathResolver`. `ActionArtifacts` realizes outputs with the current
action as provider context and requirements with the selected dependency
action as provider context:

```rust
let outputs = plan
    .output_artifacts(action)
    .into_iter()
    .map(|artifact| realize(action, artifact));

let dependencies = plan
    .dependency_artifacts(action)
    .into_iter()
    .map(|(provider, artifact)| realize(provider, artifact));
```

A `LoweredArtifact` carries the provider action ID, logical `ArtifactKey`, and
realized paths. A `LoweredAction` combines those dependency/output artifacts
with external file inputs, the concrete process command, diagnostics, and
executor policy. The n2 adapter alone registers files and constructs
`n2::Build` values.

Every lowered command retains structured argv. Response files change only its
execution transport. The first argument's resolved executable path is an
external input unless a dependency artifact provides it. External inputs are
sorted and deduplicated before n2 adaptation.

Some existing command builders also repeat a dependency artifact path in their
additional file inputs. This preserves the current n2 graph contract while the
explicit artifact dependency remains the authoritative producer edge. Removing
those redundant file inputs is a separate graph-normalization change; dry-run
or another `LoweredAction` consumer must not rely on the duplication to recover
producer ordering.

Some semantic filesystem observations cannot be represented as ordinary n2
file edges. Compiler actions carry the selected standard-library interface
bundle as a recursive `.mi` input for action identity, while the n2 adapter
omits that directory from its file list. Native lowering enumerates the chosen
toolchain include tree and attaches its headers as ordinary inputs.

Actions with broader unmodeled observations remain cache-ineligible. This
includes proof execution, documentation generation, arbitrary prebuild shell
commands, and unstructured custom compiler or linker flags. Lowering does not
infer inputs by parsing those flags.

Current dry-run still renders the n2 graph and uses retained structured argv to
recover commands hidden by response-file transport. A future dry-run can
consume `LoweredAction` and requested artifacts directly; the artifact model
does not require n2 start nodes for presentation semantics.

## Standalone script boundary

Standalone `.mbt` and `.mbtx` execution starts from one complete `BuildPlan`.
After action hydration, a projection retains dependency work as
`LoweredAction` values and lowers script-owned work into an n2 graph. Following
artifact providers to a fixed point includes package-less prerequisites such
as the runtime library and runtime objects.

The dependency graph executes first using
`standalone-dependencies.moon_db`, followed by the script graph using its mode
database. If a provider belongs to dependency preparation, the script graph
retains its realized path as an n2 input without duplicating the provider.

Ordinary project and workspace commands continue to produce and execute one
plan and one n2 graph.

## Results above lowering

Caller intent is still accepted as `BuildPlanNode` values at the compile entry
point. During plan construction those action-shaped requests are normalized to
requested `ArtifactKey` values. `LoweringResult`, `CompileOutput`, and
`BuildMeta` preserve those keys alongside their realized physical paths, so
upper layers no longer recover result meaning from provider nodes or output
positions.

For non-native backends, `LinkCore` directly provides `Executable`; there is no
planning-only `MakeExecutable` alias and every planned action lowers exactly
once. Native and LLVM plans retain the distinct `LinkedCore` intermediate and a
real `MakeExecutable` provider.

For native macOS debug targets, `Executable` and `DsymBundle` are separate
requested results with separate providers. Requesting the dSYM path causes n2
to run `GenerateDsym` without relying on a companion-path convention.

## Checks

The obsolete upper-layer models should have no matches:

```sh
rg -n '\bBuildProduct\b|\bFileDependencyKind\b' \
  crates/moonbuild-rupes-recta/src
```

The lowering boundary should consume `BuildAction`, `ArtifactKey`, and
`LoweredArtifact`; it should not reconstruct dependencies from
`BuildPlanNode` or physical output selectors.
