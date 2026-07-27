---
status: accepted
---

# Plan Standalone Dependencies Separately

Standalone script builds will plan dependency-package work separately from
script-package work. Script planning will treat the dependency products it
needs as external inputs, dependency planning will use the existing build rules
to produce those products, and the resulting script n2 graph must not contain
dependency producer nodes. Because dependency output paths are deterministic,
the script n2 graph may be lowered during the same planning invocation, but it
is executed only after dependency preparation succeeds.

The first implementation will reuse the existing build-plan construction,
`BuildProduct` vocabulary, artifact layout, and n2 lowering. Dependency outputs
will remain under `_build/.../.mooncakes`, and the dependency n2 graph will be
regenerated for each invocation while using its own persistent n2 database.
Moon will not decide freshness by scanning for missing files: the dependency
executor owns freshness checking and guarantees that requested dependency
products are materialized before script execution.

Standalone `.mbt` and `.mbtx` files built from persistent paths retain the
dependency n2 database across invocations. `moon run -e` and `moon run -` use
the same split planning path, but their synthesized temporary projects are
deleted after each invocation, so they do not currently reuse dependency work
across invocations. Stable or global cache storage for those entry points is
deferred.

## Consequences

- The initial change is limited to standalone `.mbt` and `.mbtx` builds.
- Ordinary project and workspace commands keep their existing single-plan,
  single-n2-graph planning and execution path.
- Registry resolution remains unchanged.
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
