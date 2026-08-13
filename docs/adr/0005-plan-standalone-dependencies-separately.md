---
status: accepted
---

# Prepare Standalone Dependencies Before Script Execution

Standalone script builds have two stages: prepare the required dependency
products, then plan, build, and run the synthesized script package using the
materialized outputs. The durable boundary is dependency product requirements
in and concrete outputs out; neither n2 nor a second `BuildPlan` is part of that
long-term contract.

The current incremental implementation constructs one complete logical
`BuildPlan` and one normalized `BuildActionPlan`, then projects the actions into
two disjoint n2 graphs. Package dependencies remain ordinary producer-consumer
edges rather than a second planning vocabulary. All actions owned by dependency
packages seed the dependency projection; their dependency closure also includes
any package-less shared actions they need. The remaining actions form the script
projection. A dependency action depending on a script-owned action is an invalid
phase ordering.

Dependency outputs remain under `_build/.../.mooncakes`. When their producer
actions are omitted from the script n2 graph, the same concrete output paths
remain ordinary file inputs to script actions. Moon executes the dependency
graph first, then executes the script graph. Both graphs use the target
directory's persistent n2 database; records whose outputs do not belong to the
current graph are ignored when n2 loads them. There is no file-existence scan
between the phases: n2 currently owns dependency freshness and guarantees that
the requested products are materialized.

The dependency n2 projection is a temporary preparation adapter, not a permanent
second planner. A future action-to-output implementation can replace this first
stage and feed its concrete outputs into a narrower script plan.

Standalone `.mbt` and `.mbtx` files built from persistent paths retain the
target-directory n2 database across invocations. `moon run -e` and `moon run -`
use the same split planning path, but their synthesized temporary projects are
deleted after each invocation, so they do not currently reuse dependency work
across invocations. Stable or global cache storage for those entry points is
deferred.

## Consequences

- The initial change is limited to standalone `.mbt` and `.mbtx` builds.
- Ordinary project and workspace commands keep their existing single-plan,
  single-n2-graph path.
- Registry resolution remains unchanged.
- Standalone planning uses the same complete build rules as ordinary planning.
- The temporary dependency projection is isolated after action normalization,
  where it can later be replaced without changing planning semantics.
- SHA identities, action-to-outcome caching, global storage, and a generalized
  dependency executor interface remain deferred.
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
- Projecting one complete normalized action plan is the selected temporary n2
  adapter. It keeps one source of planning truth during this incremental change
  while making dependency preparation independently replaceable.
- Designing the complete action-to-output cache interface now was rejected
  because it would expand this change before the required interface is known.
