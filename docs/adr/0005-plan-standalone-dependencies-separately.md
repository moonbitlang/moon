---
status: accepted
---

# Prepare Standalone Dependencies Before Script Execution

Standalone script builds create one logical plan, then execute it in two stages:
prepare the required dependency products, followed by building and running the
synthesized script package from those materialized outputs. The durable
boundary is dependency product requirements in and concrete outputs out;
neither n2 nor a second `BuildPlan` is part of that long-term contract.

The implementation constructs one complete logical `BuildPlan` and one
normalized `BuildActionPlan`, then projects it into retained dependency
`LoweredAction` values and one script n2 graph. Package dependencies remain
ordinary producer-consumer edges rather than a second planning vocabulary. All
actions owned by dependency packages seed the dependency projection; their
dependency closure also includes any package-less shared actions they need. The
remaining actions form the script projection. A dependency action depending on
a script-owned action is an invalid phase ordering.

Dependency outputs remain under `_build/.../.mooncakes`. When their producer
actions are omitted from the script n2 graph, the same concrete output paths
remain ordinary file inputs to script actions. Moon first runs dependency
preparation, whose success guarantees that every required product exists at its
lowered path, then executes the script graph. The script graph does not know
whether preparation restored an output or executed its producer.

With the build cache enabled, preparation computes canonical identities from
the retained actions, restores complete validated output sets, sends only
misses through a controlled `LoweredAction`-to-n2 adapter, and publishes
successful outputs. The miss executor uses temporary n2 state. With the build
cache disabled, all retained dependency actions go through the same adapter and
keep the existing persistent `standalone-dependencies.moon_db`.

The dependency n2 projection is a miss-execution adapter, not a permanent
second planner. The preparation contract can outlive that adapter and the
current action-to-output store.

Standalone `.mbt` and `.mbtx` files built from persistent paths can reuse the
global action-output store. `moon run -e` and `moon run -` use the same
preparation path, but the initial identity hashes concrete paths exactly.
Their changing synthesized temporary paths may therefore cause conservative
misses. Relocatable identities are deliberately deferred until command path
semantics can be modeled without textual rewriting.

## Consequences

- The initial change is limited to standalone `.mbt` and `.mbtx` builds.
- Ordinary project and workspace commands keep their existing single-plan,
  single-n2-graph path.
- Registry resolution remains unchanged.
- Standalone planning uses the same complete build rules as ordinary planning.
- Dependency preparation is isolated after action normalization. Cache
  restoration and n2 miss execution remain hidden behind its
  "materialize all required products" contract.
- The first global action-to-output store is limited to regular-file outputs
  needed by standalone dependencies.
- Splitting execution introduces a phase barrier. Cold and warm performance
  must be measured against the single-graph implementation.

## Considered Options

- Two independent `BuildPlan` constructors were rejected because they duplicate
  one semantic graph and require special "external product" edges and paths to
  reconnect it. They also make ownership of package-less shared actions
  ambiguous.
- Splitting or detaching nodes after n2 lowering was rejected because the phase
  boundary would be expressed through executor graph surgery rather than
  normalized build actions.
- Projecting one complete normalized action plan is the selected boundary. It
  keeps one source of planning truth while making dependency preparation
  independently replaceable.
- Exposing n2 graph-builder state to the cache was rejected. Only selected
  lowered misses cross the controlled adapter boundary.
