---
status: accepted
---

# Plan Standalone Dependencies Separately

Standalone script builds will plan dependency-package work separately from
script-package work. Script planning will treat the dependency products it
needs as external inputs, dependency planning will use the existing build rules
to produce those products, and the script plan will be lowered and executed
only after dependency preparation succeeds. The resulting script n2 graph must
not contain dependency producer nodes.

The first implementation will reuse the existing build-plan construction,
`BuildProduct` vocabulary, artifact layout, and n2 lowering. Dependency outputs
will remain under `_build/.../.mooncakes`, and the dependency n2 graph will be
regenerated for each invocation while using its own persistent n2 database.
Moon will not decide freshness by scanning for missing files: the dependency
executor owns freshness checking and guarantees that requested dependency
products are materialized before script execution.

## Consequences

- The initial change is limited to standalone `.mbt` and `.mbtx` builds.
- Normal workspace build planning and registry resolution remain unchanged.
- The two planning paths share package build rules instead of duplicating them.
- SHA identities, action-to-outcome caching, global storage, and a generalized
  dependency executor interface are deferred until the split exposes concrete
  variation that requires those abstractions.
- Splitting execution introduces a phase barrier. Cold and warm performance
  must be measured against the single-graph implementation.

## Considered Options

- Building one complete n2 graph and then tagging, cloning, or detaching
  dependency producers was rejected because dependency preparation would remain
  an implementation detail extracted from script planning rather than an
  independent planning use case.
- Designing the complete action-to-outcome cache interface before splitting the
  planners was rejected because it would expand the change without evidence
  about the interface the later cache implementation actually needs.
