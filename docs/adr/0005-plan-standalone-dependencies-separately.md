---
status: accepted
---

# Prepare Dependency Actions Before Single-File Script Execution

Single-file builds have two execution phases: materialize dependency products,
then build and run the synthesized script package against those concrete
outputs. The durable boundary is a sequence of `LoweredAction` values in and
validated output files out. Neither n2 nor a second logical planner is part of
that contract.

Rupes Recta constructs one `BuildPlan` and one `BuildActionPlan`. Dependency
actions are retained as `LoweredAction` values after paths, commands, external
inputs, and response files have been selected. Script actions continue directly
into the ordinary n2 adapter. The retained actions preserve producer references
and logical products, so Moon does not recover dependencies by inspecting an n2
graph.

Moon identifies each retained action from its lowered semantics and recursively
from the identities of its producers. A valid hit materializes every output of
the action. Only misses are passed through a controlled
`LoweredAction -> n2 graph` adapter, using per-invocation n2 state. The adapter
exists only to execute selected misses; the cache neither reads nor rewrites an
n2 graph.

`BuildActionId` is local to one lowering operation. It may locate a producer
while identities are built, but it is never serialized or hashed as persistent
identity. Dependency identity instead includes the producer digest, logical
product, and realized paths.

Dependency artifacts keep their existing concrete paths under
`_build/.../.mooncakes`. Local action records and immutable output objects live
under `_build/.mooncakes/.build-cache`. The script n2 graph sees materialized
dependency paths as ordinary file inputs whose producers are outside that graph.

## Consequences

- The cache is limited to dependency preparation for single-file builds.
- Ordinary project and workspace commands keep their existing single-plan,
  single-n2-graph path.
- Registry resolution remains unchanged.
- Single-file planning uses the same complete build rules as ordinary planning.
- The dependency phase can replace its local store or miss executor without
  changing logical planning or the script phase.
- The local store is not the configured global build cache and does not provide
  cross-project reuse.
- Complete builds that share one `_build` still rely on Moon's target-directory
  lock. Store publication handles same-object writer races, but this does not
  make the rest of a shared build tree independently concurrent.
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
- Keeping a persistent dependency n2 graph was rejected because n2 treats
  materialized outputs whose producers are absent as uncached source files.
  Making that graph the cache source of truth would also couple action identity
  to executor representation.
- Retaining lowered dependency actions is selected because it preserves one
  lowering truth while keeping the cache and the miss executor replaceable.
- Generalizing the store to ordinary project builds or global cross-project
  reuse is deferred until the single-file boundary has production evidence.
