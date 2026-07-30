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
    RuntimeObject { index: u32 },
    RuntimeLib,
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
The runtime archive follows the same contract with `RuntimeObject`
dependencies and a `RuntimeLib` output. Optional prebuilt SIMDUTF objects in
release builds are selected during planning and become external file inputs of
the archive action because the build plan does not produce them. Planning also
computes the runtime archive's member-list fingerprint once; product
realization uses it in the static archive path, so consumers do not recalculate
the fingerprint.

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

At this common boundary, a structured command contributes its first argument,
the concrete executable path, to the action's external file inputs. External
inputs are sorted and deduplicated before the action reaches n2. This uses the
original structured arguments even when command transport switches to a
response file. Verbatim shell commands remain opaque and do not contribute an
inferred executable.

This keeps responsibilities separate:

- `BuildPlan` owns graph edges, coalescing, and planning-only terminology.
- `BuildActionPlan` owns the normalized action/product interface between phases.
- `ArtifactPathResolver` owns logical product to path resolution.
- action lowering owns concrete products, external inputs, and commands.
- the n2 adapter owns n2 graph construction.

## Standalone script boundary

Standalone `.mbt` and `.mbtx` execution starts from one complete `BuildPlan`.
Package dependencies use the same edges as ordinary project compilation. After
the plan is normalized once, an action-level projection separates it into two
disjoint n2 graphs.

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
are removed after each invocation, so they do not currently reuse the
dependency database across invocations. Stable or global cache storage for
those entry points remains future work.

Ordinary project and workspace commands continue to produce and execute one
plan and one n2 graph.

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
