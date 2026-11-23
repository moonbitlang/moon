---
status: accepted
---

# Select Build Configuration Before Package Resolution

MoonBuild selects a Build Configuration before Package Resolution and retains
that selection through planning, lowering, artifact layout, and build results.
The configuration currently consists of the Target Backend and debug/release
profile. Selecting the profile only while preparing compiler options leaves
the initial command request without an explicit profile to retain through
source selection, planning, and build results.

Command policy resolves explicit options and defaults once. Package Resolution
receives only the selected backends, and each Build Plan consumes its backend's
Package Relations. Later phases may derive action-specific options from the
configuration, but do not choose its backend or profile again. For example,
retaining debug symbols in a release build does not select debug-only sources.

## Ownership and Consequences

- The CLI owns command defaults and requested configurations. `moonbuild`
  prepares external inputs when needed and orchestrates the build. RR owns
  Package Resolution, planning, lowering, and artifact layout; it receives
  semantic choices independently of CLI parsing types.
- The early configuration contains the choices needed to interpret the
  project. Prepared tools, filesystem observations, and effective compiler
  options remain in their existing owners and are supplied when their phases
  need them. Native toolchain and payload selection retain the ownership
  established by [ADR 0002](0002-native-abi-policy-belongs-to-toolchains.md).
- `CompileConfig` retains the selected configuration, and preparation derives
  layout inputs from it while checking that prepared backend resources match.
  Build results retain the same selection for metadata, runtime selection,
  and profiling.
- Resolution handles only requested backends. Within a discovered project,
  imports and Package Relations currently do not depend on profile, so their interface
  and map key remain `TargetBackend`. Each phase consumes the part of the
  selected configuration that affects its result; selecting a configuration
  early does not make every result depend on the entire configuration.

The CLI selects one effective profile per invocation. This change does not add
simultaneous debug/release requests or profile-dependent imports.

Configuration scope does not replace action identity or cache inputs. Compiler
arguments and tool inputs can differ within one profile. This decision also
does not establish new target-platform fields, a configuration matrix CLI, or
new output-directory conventions.

Layout API enforcement is a separate follow-up: bind the selected configuration
to the layout, remove per-call backend arguments, and validate the prepared
configuration and paths together before exposing them read-only. Until then,
callers derive the existing layout arguments from the selected configuration.
Formatting does not select a Build Configuration and must retain its existing
output paths when that layout interface changes.

## Considered Options

- Keeping profile selection late retains the existing timing, but leaves the
  command request incomplete until compiler options are prepared. Selecting it
  earlier is independent of the backend-only package-relationship key.
- Keying Package Relations by the full configuration would introduce a profile
  distinction that import semantics do not have. Future extensions should add
  key dimensions only when those dimensions affect package relationships, such
  as OS if import conditions gain OS predicates. This does not fix the resolver
  interface permanently to backend alone.
- Passing the entire `CompileConfig` into Package Resolution would couple early
  semantic choices to backend resources and physical paths needed by later
  phases. Keeping the early selection small preserves demand-driven preparation.
- Independently selecting backend/profile in each phase makes consistency a
  convention at every call site. Passing the original selection removes that
  repeated decision; the layout follow-up will also enforce the relationship
  through its interface.
