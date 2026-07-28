# Build plan product dependencies

Rupes Recta now uses a compiler-style split between planning, the action-level
plan consumed by lowering, and backend graph construction:

```text
BuildPlan       = planning IR: graph nodes, edge semantics, coalescing
BuildActionPlan = action-level build plan: action ids, hydrated action data, logical products
LoweredAction   = concrete action: realized products, paths, and process command
n2::Build       = executor representation produced by the n2 adapter
```

The important invariant is that `build_lower` consumes `BuildActionPlan` only. It
does not import or match on planning internals such as `BuildPlanNode`,
`FileDependencyKind`, or `PlanArtifactNeed`.

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

`BuildPlan` still owns graph traversal, action coalescing, and edge semantics.
Edges describe what logical output a consumer needs from a producer:

```rust
pub enum FileDependencyKind {
    AllFiles,
    Artifacts(PlanArtifactNeed),
    ProofArtifacts { mi: bool, mlw: bool, report: bool },
    GenerateTestInfo { meta: bool },
}

pub enum PlanArtifactNeed {
    Interface,
    CoreIr,
    InterfaceAndCoreIr,
}
```

`CoreIr` names the compiler IR artifact written with the `.core` extension. It
is not related to the `moonbitlang/core` package.

Builders request logical needs instead of path-shaped outputs. `Check`
dependencies need only the interface. Normal downstream `BuildCore`
dependencies also track Core IR as an n2 input, so a dependency implementation
change that leaves the interface stable still rebuilds the dependent package:

```rust
let edge = match dep_node {
    BuildPlanNode::Check(_) => FileDependencyKind::Artifacts(PlanArtifactNeed::Interface),
    BuildPlanNode::BuildCore(_) if check_only => {
        FileDependencyKind::Artifacts(PlanArtifactNeed::Interface)
    }
    BuildPlanNode::BuildCore(_) => {
        FileDependencyKind::Artifacts(PlanArtifactNeed::InterfaceAndCoreIr)
    }
    BuildPlanNode::BuildVirtual(_) => FileDependencyKind::AllFiles,
    _ => unreachable!(
        "need_interface_of_dep only schedules Check, BuildCore or BuildVirtual"
    ),
};
self.add_edge_spec(node, dep_node, edge);
```

When `Check(target)` is coalesced into `BuildCore(target)`, `BuildPlan`
converts broad `Check` edges to the logical interface need that `BuildCore` can
satisfy:

```rust
fn edge_for_coalesced_check(edge: FileDependencyKind) -> FileDependencyKind {
    match edge {
        FileDependencyKind::AllFiles => FileDependencyKind::Artifacts(PlanArtifactNeed::Interface),
        FileDependencyKind::Artifacts(need) => {
            assert!(need.is_subset_of(PlanArtifactNeed::Interface));
            FileDependencyKind::Artifacts(need)
        }
        _ => panic!("Check edges can only request logical artifacts"),
    }
}
```

## Build Action Plan

`BuildPlan::build_action_plan()` creates the view consumed by backend lowering:

```rust
pub struct BuildActionId(usize);

pub enum BuildAction<'a> {
    Check { target: BuildTarget, info: &'a BuildTargetInfo },
    BuildCore { target: BuildTarget, info: &'a BuildTargetInfo },
    LinkCore { target: BuildTarget, info: &'a LinkCoreInfo, make_executable_info: Option<&'a MakeExecutableInfo> },
    MakeExecutable { target: BuildTarget, info: Option<&'a MakeExecutableInfo> },
    // other action variants carry the same hydrated planning metadata
}
```

`MakeExecutableInfo` is present only for native executable work. For non-native
backends, `MakeExecutable` remains a final-artifact alias over `LinkCore` and is
a no-op in backend lowering.

Logical outputs are exposed as producer-free `BuildProduct` values:

```rust
pub enum BuildProduct {
    PackageInterface { target: BuildTarget },
    PackageCoreIr { target: BuildTarget },
    GeneratedTestDriver { target: BuildTarget },
    CStubObject { package: PackageId, index: u32 },
    PrebuildOutputPath { path: PathBuf },
    // other logical outputs
}
```

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

Dependency edge selectors become `(dependency action, product)` pairs. The
dependency action is edge context for path resolution; it is not stored inside
the product:

```rust
pub fn dependency_products(&self, id: BuildActionId) -> Vec<(BuildActionId, BuildProduct)> {
    self.plan
        .dependency_edges(self.node(id))
        .flat_map(|(node, kind)| {
            let dependency_action = self.id_for_node(node);
            self.products_for_edge(node, kind)
                .into_iter()
                .map(move |product| (dependency_action, product))
        })
        .collect()
}
```

For example, an archive/link C-stub action no longer scans raw build-plan
edges in `build_lower`. Its object inputs are exposed by `BuildActionPlan` as
`(object_action, BuildProduct::CStubObject { ... })` dependencies.

## Action Lowering and n2 Adaptation

`build_lower` matches on `BuildAction` and resolves `BuildProduct` paths
through `ArtifactPathResolver`:

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

This keeps responsibilities separate:

- `BuildPlan` owns graph edges, coalescing, and planning-only terminology.
- `BuildActionPlan` owns the normalized action/product interface between phases.
- `ArtifactPathResolver` owns logical product to path resolution.
- action lowering owns concrete products, external inputs, and commands.
- the n2 adapter owns n2 graph construction.

## Single-file dependency boundary

Single-file `.mbt` and `.mbtx` execution starts from one complete `BuildPlan`.
Package dependencies use the same edges as ordinary project compilation. After
normalization, dependency actions are retained as `LoweredAction` values while
script actions continue into the ordinary n2 adapter.

A `LoweredAction` is the complete concrete description consumed by dependency
preparation:

- structured command arguments or an explicitly verbatim command;
- working directory, normalized environment, and response-file data;
- external inputs with their content semantics;
- dependency products with producer action, logical product, and concrete
  paths; and
- output products and concrete output paths.

Rupes Recta owns this description. Moon owns identity, restore, miss selection,
execution, and publication. The only reverse direction is a controlled adapter
that converts the selected misses into an n2 graph. Moon does not receive
`N2GraphBuilder`, inspect n2 file edges, or copy part of an existing n2 graph.

`BuildActionId` is valid only within the current lowering. Moon uses it to find
the retained producer and recursively substitutes the producer action digest.
The persisted identity contains no numeric action id. Dependencies are ordered
by their complete product fingerprint, and external inputs and output paths are
sorted, so identity does not depend on map, set, or traversal order.

The script graph keeps the original realized dependency paths. A hit
materializes those paths directly. A miss graph treats a dependency supplied by
a hit as an ordinary file input; it does not need the hit producer action.
Selected misses use per-invocation n2 database state under the local store, so
n2 is an execution adapter rather than the dependency freshness authority.

Ordinary project and workspace commands continue to lower each action directly
into one n2 graph without retaining all lowered actions. `moon run -e` and
`moon run -` use the same single-file dependency path; whether their local store
survives is determined by the lifetime of their synthesized target directory.

## Compatibility

`LoweringResult` returns root action artifacts as `(BuildActionId, paths)` pairs
in input action order. The compile layer re-keys those artifacts back to
`BuildPlanNode` for the existing public `CompileOutput` shape. That keeps
compatibility above lowering while proving that backend lowering no longer sees
planning internals.

## Checks

The boundary can be checked with:

```sh
rg -n '\bBuildPlan\b|BuildPlanNode|FileDependencyKind|PlanArtifact' \
  crates/moonbuild-rupes-recta/src/build_lower
```

There should be no meaningful matches.
