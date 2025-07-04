- Feature Name: `rupes-recta`
- Start Date: 2025-06-04
- RFC PR: N/A
- Issue: [#802](https://github.com/moonbitlang/moon/issues/802)

# Summary

[summary]: #summary

Document and simplify the existing behavior.
Perform a major refactor and rewrite of MoonBuild,
mainly around the dependency graph and build execution part,
in order to prevent clutter, ease development and reduce bugs.

# Motivation

[motivation]: #motivation

The current MoonBuild codebase is not very clean, to say the least.
We have seen multiple bugs related to suboptimal code structuring,
such as [#838](https://github.com/moonbitlang/moon/pull/838),
[#829](https://github.com/moonbitlang/moon/pull/829),
[#778](https://github.com/moonbitlang/moon/pull/778),
and many others.

The codebase bloat also significantly blocks feature addition and fixes,
as the changeset of relevant features are scattered in multiple files around the codebase,
making contributing to the project less efficient for both internal and external contributors.

For example, to generate the build commands for a package (compile unit), we need to:

1. Find the containing module (distribution unit) in `ResolvedEnv`.
2. Find the package in the (badly named) `ModuleDB` that contains each package that can be built.
3. Convert the package defintion to `BuildDepItem`, `BuildLinkDepItem` and `LinkDepItems`s
   according to various rules.
   Different build stages also sometimes share the types used.
4. Convert the `*Item`s into `n2`'s build graph input, with even more rules.
   Not all information of present in each item is used,
   and there's no clear rules on which conversion uses which information.
5. Run the build graph using `n2`.

This process introduces multiple indirection with suboptimal abstractions.

Additionally, the usage of `n2` is holding us back:
We do not need the flexible build graph provided by `n2` --
our inputs and outputs are fixed, a package-based build graph will be good enough.
And `n2` needs every build step to be a subprocess call,
meaning that we will need to perform things already done in the current `moon` process,
like syncing dependencies, parsing project structure, etc. repeatedly across multiple `moon` subprocesses.
This is also a source of potential racing condition bugs like
[#838](https://github.com/moonbitlang/moon/issues/838).
Keeping everything in a single process will make life easier.

Finally, although fixes and feature additions in the near future is still managable,
in order to keep a sustainable development speed and reduce bug rate,
it would be vastly better if we can refactor the codebase into a more ordered state.

A major refactor and even rewrite is, thus, imminent.

# Impact Assessment

Although being a major refactor,
this refactor should have little impact to existing code.
There might be breaking change to
how specific, target-dependent features interact with the build system,
but overall most existing code should still work.

The refactored/rewritten part will be feature-gated until it's complete,
so the user will not see half-finished features in day-to-day usage.

# Migration Plan

The refactor will be performed in the following order:

1. Document the existing behavior of code.
   This gives us a foundation on what our build system currently do.

2. Add auxiliary features that aid the migration process:

   - Feature gating
   - Working logging system
   - Tests that ensures both versions perform the same

3. Perform the main design and migration

   - Rewrite the core build graph,
     remove dependency on `n2`,
     and use a build graph executor specific to MoonBit instead.
   - Reimplement data structures that fit our usage

4. Fix any unexpected incompatibility and test failures with the existing design.

5. Flip the feature gate and make the new implementation the default.

6. Remove the legacy implementation once the new one is bug-free and widely adopted.

# Drawbacks

[drawbacks]: #drawbacks

- The migration duration is unspecified.
  A rough estimate will be approximately 1 man-month, but actual time may vary greatly.
- Every new feature added in this period will need to be reflected twice,
  once in the legacy implementation and once in the new one.

# Rationale and alternatives

[rationale-and-alternatives]: #rationale-and-alternatives

- What other designs have been considered and what is the rationale for not choosing them?
  - Incremental fixes might be possible,
    but untangling the existing logic code might not be easier than just a brand new rewrite.
  - Removing `n2` dependency will be a major change,
    so we can take the opportunity to clean more things up.
- What is the impact of not doing this?
  - Long-term maintainability will be affected. New features will take longer to land.

# Prior art

[prior-art]: #prior-art

TODO

# Unresolved questions

[unresolved-questions]: #unresolved-questions

TODO

# Future possibilities

[future-possibilities]: #future-possibilities

- Regular refactors / lower refactor threshold?
- RFCs for every major feature change?
