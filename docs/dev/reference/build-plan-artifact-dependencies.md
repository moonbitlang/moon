# Build plan artifact dependencies

Rupes Recta uses one logical artifact dependency model in Build Plan. The
current lowering representation realizes those artifacts as concrete paths:

```text
BuildPlan     = requested artifacts + provider actions + artifact requirements
build_lower   = command construction + physical artifact realization
ExecutionPlan = ActionId actions + concrete input/output paths + requested artifact results
n2::Build     = executor representation produced by the n2 adapter
```

The key invariant is that an action depends on artifacts, not on another
action's position or an output selector attached to an action edge. Provider
selection is a planning concern. Lowering receives the selected provider only
as the context needed to realize the artifact's physical path.

## Backend configuration

After command adapters resolve the Target Backend and expand user intent to
requested `ArtifactKey` values, Moon selects one `BackendConfig` value:

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
test generation, linking, native support, and documentation results.
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

    // other logical artifacts
}
```

`CoreIr` names the compiler IR artifact written with the `.core` extension. It
is unrelated to the `moonbitlang/core` package.

Artifact identity never contains an execution `ActionId` or `_build` root.
`CheckMi`, `BuildMi`, and `VirtualContractMi` are distinct because they are not
interchangeable compiler inputs even though they all currently use `.mi`.
`EmitProof` and `Prove` are alternative providers of the same `ProofMi` and
`ProofWhyml` artifacts; only `Prove` additionally provides `ProofReport`.
Provider selection belongs to the invocation lifecycle, not artifact identity.

C-stub and runtime library artifacts are realized as static archives.
`RuntimeObject` identifies each separately compiled runtime translation unit
consumed by the runtime archive.

`Executable { package, target_kind }` includes test executables: source,
inline-test, whitebox-test, and blackbox-test targets have different
`TargetKind` values. Backend and profile remain configuration scope rather than
being repeated in every package artifact.

C-stub artifact keys use normalized declaration paths rather than list indices;
their source paths are relative to the package root. Runtime-object keys use
their stable path within the toolchain library layout, such as `runtime/foo.c`,
rather than the installed toolchain root. Ordinary package sources and
prebuild outputs remain concrete paths in package file sets rather than Build
Artifacts.

An output needs an artifact key only when Build Plan selects or consumes it as
an independently meaningful result. Merely declaring a physical output to n2
or a cache does not give it Build Artifact identity. Incidental compiler side
files such as source maps or declaration files remain part of their producing
action's physical behavior until Moon can independently request or consume
them.

A dSYM bundle is a declared physical output of the `GenerateDsym` execution
action, not an `ArtifactKey`. Its concrete path lets n2, dry-run, and cache
consumers track it without exposing it as a caller-requested Build Artifact.

## Planning IR

`BuildPlan` composes two action-owning subplans with one artifact registry:

```rust
pub struct BuildPlan {
    backend: BackendPlan,
    package_prebuild: PackagePrebuildPlan,
    artifacts: ArtifactRegistry,
    requested_artifacts: IndexSet<ArtifactKey>,
}

struct BackendPlan {
    actions: IndexSet<BuildPlanNode>,
    // metadata required to lower backend actions
}

struct PackagePrebuildPlan {
    actions: IndexMap<PackagePrebuildKey, PackagePrebuildAction>,
}

enum BuildPlanActionKey {
    Backend(BuildPlanNode),
    PackagePrebuild(PackagePrebuildKey),
}

struct ArtifactRegistry {
    providers: HashMap<ArtifactKey, BuildPlanNode>,
    artifacts_by_provider: HashMap<BuildPlanNode, IndexSet<ArtifactKey>>,
    requirements_by_consumer: HashMap<BuildPlanNode, IndexSet<ArtifactKey>>,
}
```

There is no weighted action-edge graph and no `FileDependencyKind`. A provider
may expose multiple artifacts, but each artifact has at most one provider in a
plan. At the end of planning, validation requires every requested artifact and
every Artifact Requirement to have a provider.

`BuildPlanActionKey` is the action union used while lowering the two subplans.
Backend nodes retain `BuildTarget` and therefore `TargetKind` only where those
concepts apply. Package prebuild actions are keyed by their declaration
coordinate: custom commands use the manifest declaration index, while moonlex
and moonyacc use their concrete input paths. The artifact registry currently
contains only semantic backend artifacts; concrete package-file relationships
are resolved after lowering.

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

The same rule handles invocation roots. `UserIntent` names results such as
`CheckMi`, `CoreIr`, or `Executable`; it does not construct provider actions.
For example, `Executable` selects `LinkCore` for Wasm/JS backends and
`MakeExecutable` for native/LLVM backends. The complete caller-requested
artifact set is recorded before provider expansion, so proof-surface artifacts
select `Prove` when the invocation also requests `ProofReport`, and otherwise
select `EmitProof`.

JavaScript test intents also request one `NodeTestPackageConfig` for each
package. Its dedicated package-scoped `GenerateNodeTestPackageConfig` provider
lowers to `moon tool generate-node-test-package-config`, which writes the empty
`package.json` that prevents Node from inheriting module settings from the
user's project. Test-driver actions do not own this artifact; whitebox,
blackbox, and inline test targets share the same package boundary.

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

Each backend-specific `BuildPlan` composes a `BackendPlan` and a
`PackagePrebuildPlan`. The package-prebuild subplan owns complete custom
prebuild, moonlex, and moonyacc actions; it is not a second semantic artifact
model.

Package discovery and prebuild declarations contribute concrete paths to the
package file sets used by Build Target Projection. A path does not gain a
different identity depending on whether discovery already observed it or a
prebuild action declares it as an output. Lowering places those paths in the
consumer's input observations and declares the producing prebuild action's
outputs. Execution Plan consumers resolve matching input and output paths to
the same producer relationship, including custom-to-moonlex and
custom-to-moonyacc pipelines.

Unconsumed package prebuild actions remain planned under the existing
semantics. Their outputs become n2 start nodes because no later action consumes
them.

The prebuild action retains its resolved execution paths and cwd. Its output
paths are physical execution behavior, not semantic `ArtifactKey` values.

## Execution Plan

Lowering consumes `BuildPlan` directly and inserts complete `ExecutionAction`
values into the Execution Plan. A borrowed `BuildAction` value may be hydrated
on demand while constructing a command, but there is no stored
`BuildActionPlan`, draft execution action, or second action topology between
semantic planning and execution lowering.

`ExecutionPlan` assigns an `ActionId` to each concrete `ExecutionAction`.
Declared outputs are instead keyed by their concrete paths, which are already
required to be unique within the plan. An execution action has one collection
of `InputObservation` values. A regular file observation resolves to a
producer when its path matches a declared output; an unmatched path is
supplied across the Execution Plan boundary. This distinction is therefore a
property of the completed graph, not something each command builder must
classify. Every declared output retains its producer `ActionId` and optional
`ArtifactKey`, so adapters and identity consumers can follow the relationship
without another dependency object, output arena, or plan-wide artifact
registry. One action may provide several Build Artifacts, and a Build Artifact
may realize to one or several physical outputs.

The builder's artifact registry exists only while actions are inserted. It
annotates declared outputs and resolves requested artifacts to output paths for
command results. Artifact requirements have already been realized as concrete
input observations before insertion. The final Execution Plan therefore does
not impose one global `ArtifactKey` namespace on future composition of
independently scoped Build Plans.

`moon fmt` is a Lightweight Command with no logical Build Artifact selection,
so it constructs complete Execution Actions directly after its lightweight
project discovery. It does not synthesize a Build Plan, but shares the same
Execution Plan, n2 adapter, dry-run, and execution path.

Multi-backend invocations keep one independently scoped Build Plan and
Execution Plan per backend through lowering. The command layer then composes
the Execution Plans before n2 adaptation. Action IDs are remapped into the
combined plan; Build Artifact annotations remain plan-scoped metadata on their
physical outputs and are not used as a cross-backend namespace.

Concrete output paths are the composition boundary. If two plans declare the
same physical output, composition shares the provider only when the complete
Execution Action, its complete output set, and all output annotations agree.
Partial overlap or different execution behavior is rejected before n2 sees the
graph. Package prebuild actions therefore collapse naturally without a
prebuild-specific merge rule, while backend outputs remain separate under
their backend-specific paths.

Physical-only declared outputs carry `artifact: None`. They still participate
in executor roots and cache identity, but do not become user-requestable Build
Artifacts. `ActionId` is a process-local arena handle, not a persistent content
identity or cache digest. Concrete output paths likewise are not cache digests.

## Action lowering and n2 adaptation

`build_lower` walks semantic `BuildPlanActionKey` values, hydrates the metadata
for one action on demand, and resolves every `ArtifactKey` through
`ArtifactPathResolver`. `ActionArtifacts` realizes outputs with the current
action as provider context and requirements with the selected provider action as
context:

```rust
let outputs = plan
    .provided_artifacts(node)
    .map(|artifact| realize(action, artifact));

let dependencies = plan
    .artifact_dependencies(node)
    .map(|(provider, artifact)| realize(provider, artifact));
```

The `ExecutionPlanBuilder` registers each realized semantic output, assigns
`ActionId` handles, and rejects duplicate artifact providers or physical-output
paths. An `ExecutionAction` combines input observations, declared outputs, the
concrete process command, diagnostics, and executor/cache policy. Consumers
resolve a regular input path through the plan's declared-output index; the n2
adapter alone registers files and constructs `n2::Build` values.

Every lowered command retains structured argv. Response files change only its
execution transport. The first argument's resolved executable path is another
input observation unless a dependency artifact already supplies that path.
Input observations are sorted and required to be unique before n2 adaptation.
A realized artifact dependency is the sole input declaration for its physical
path; command metadata such as `-check-mi` and `-impl-virtual` describes how
the compiler uses that local path without declaring it again. An installed
standard-library virtual contract has no provider in the current Build Plan,
so lowering also retains its exact interface file as an ordinary n2 input.

Some semantic filesystem observations cannot be represented as ordinary n2
file edges. Compiler actions carry the selected standard-library interface
bundle as a recursive `.mi` input for action identity, while the n2 adapter
omits that directory from its file list. Exact standard-library virtual
contracts used by `-check-mi` or `-impl-virtual` therefore remain separate file
observations for n2. Native lowering enumerates the chosen toolchain include
tree and attaches its headers as ordinary inputs.

Actions with broader unmodeled observations remain cache-ineligible. This
includes proof execution, documentation generation, arbitrary prebuild shell
commands, and unstructured custom compiler or linker flags. Lowering does not
infer inputs by parsing those flags.

Current dry-run still renders the n2 graph and uses retained structured argv to
recover commands hidden by response-file transport. A future dry-run can
consume `ExecutionPlan` directly: each input path resolves to its declared
output and producer action, while requested artifacts and physical-only outputs
retain the distinct root semantics that n2 otherwise flattens into file IDs.

## Standalone script boundary

Standalone `.mbt` and `.mbtx` execution starts from one complete `BuildPlan`
and lowers it once into one `ExecutionPlan`. Dependency preparation and script
execution are two `ActionId` selections over that plan. Following artifact
providers to a fixed point includes package-less prerequisites such as the
runtime library and runtime objects.

The dependency graph executes first, followed by the script graph. Both use the
target directory's `.moon_db`; n2 ignores records whose outputs do not belong
to the graph it is currently loading. If a provider belongs to dependency
preparation, the script graph retains its realized path as an n2 input without
duplicating the provider.

Ordinary project and workspace commands execute one n2 graph per invocation.
A single-backend invocation projects one Execution Plan directly; a
multi-backend invocation first composes its independently lowered Execution
Plans as described above. The n2 adapter preserves each Build ID's originating
Action ID, which execution projects through the composed plan's action-backend
map. n2 reports completed action output with that Build ID. This lets
JSON-formatted `moon check` execute the same composed graph while the command
layer still annotates each compiler diagnostic with its backend.

The n2 failure budget applies to the composed invocation-wide graph rather
than restarting for each backend. JSON output reports every diagnostic that
completed before that executor boundary; diagnostic-limit summaries include
machine-readable hidden error and warning counts when output was truncated.

## Results above lowering

Caller intent is accepted as `ArtifactKey` values at the compile entry point.
Plan construction selects provider actions from those keys. `ExecutionPlan`,
`CompileOutput`, and `BuildMeta` preserve the keys alongside their realized
physical paths, so upper layers do not recover result meaning from provider
nodes or output positions.

For non-native backends, `LinkCore` directly provides `Executable`; there is no
planning-only `MakeExecutable` alias and every planned action lowers exactly
once. Native and LLVM plans retain the distinct `LinkedCore` intermediate and a
real `MakeExecutable` provider.

For native macOS debug targets, `Executable` remains the requested Build
Artifact. Planning also includes `GenerateDsym`; its unconsumed concrete output
is an n2 start node, so ordinary execution and current dry-run still run
`dsymutil` without exposing the dSYM directory as a caller result.

## Checks

The obsolete upper-layer models should have no matches:

```sh
rg -n '\bBuildProduct\b|\bFileDependencyKind\b|\bBuildActionPlan\b|\bLoweredAction\b' \
  crates/moonbuild-rupes-recta/src
```

The lowering boundary should consume `BuildPlan` and produce `ExecutionPlan`.
Adapters should consume `ExecutionPlan`; they should not reconstruct semantic
dependencies from physical path collisions or n2 start-node behavior.
