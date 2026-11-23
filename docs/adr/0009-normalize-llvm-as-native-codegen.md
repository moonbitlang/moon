---
status: accepted
---

# Normalize LLVM as Native Code Generation

The `llvm` selection requests native objects generated through LLVM. Treating it
as a separate prepared backend duplicates Native platform and resource settings
and makes the generator appear to define a different kind of executable.

Build Configuration represents this selection as
`Native { codegen: Llvm }`. The existing `native` selection becomes
`Native { codegen: Auto }`. Automatic selection retains the current generated-C
and direct-object policy; LLVM remains explicit. Planning resolves that request
once and lowering consumes the resulting native mode. Explicit LLVM requests
never fall back to generated C because of profile or package compiler flags.

Native and LLVM preparation share platform, allocator, and compiler paths. Their
planned objects and executables use the same artifact forms. Generation commands,
debug support, runtime options, and final-link requirements retain their existing
differences; a shared native family does not imply interchangeable toolchains.
[ADR 0002](0002-native-abi-policy-belongs-to-toolchains.md) continues to own ABI
and CRT policy.

## Compatibility and Phase Inputs

The public `llvm` spelling, compiler protocol, conditional imports and files,
exact `supported_targets` sets, metadata, installed bundles, and `_build/llvm`
paths retain their current semantics. Build Configuration derives the existing
`TargetBackend` selector when those interfaces need it, instead of storing a
second independently editable backend. LLVM-only conditions remain distinct
from Native conditions and read the request before toolchain resolution.

Package Relations remain keyed by that selector: it already captures the Native
versus explicit LLVM distinction that imports can observe. Profiles and late
automatic payload decisions do not enter that key. The interface can gain other
dimensions when import conditions actually depend on them.

This decision does not establish cross-compilation support, new public codegen
options, or shared Native/LLVM `.mi`, `.core`, and runtime artifacts. Those need
their own capability and compatibility contracts. WAT remains an output-format
selection; the current binary-only Wasm linker is not a model constraint.
