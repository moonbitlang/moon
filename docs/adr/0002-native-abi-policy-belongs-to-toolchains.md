# Native ABI Policy Belongs to Toolchains

Native builds have separate choices that are easy to conflate: the native payload form produced by MoonBit, the executable realization used by lowering, and the native toolchain used by the host compiler/linker. We will keep backend mode focused on payload and realization choices, while the toolchain owns ABI family and CRT linkage policy. This keeps MSVC invariants local: once an MSVC toolchain is selected, runtime compilation, C stub compilation, and final executable linking all consume the same ABI and CRT contract.

## Decisions

- Direct object mode records the native target triple, not an MSVC-specific backend mode.
- Build planning resolves native compiler/linker selections into toolchains before lowering.
- Windows direct object mode for `x86_64-pc-windows-msvc` requires a cl-compatible MSVC toolchain.
- Generated-C native builds also consume toolchains, so MSVC CRT policy is not limited to the direct object path.
- Lowering must branch on the toolchain for MSVC-specific runtime, C stub, and executable-link behavior, not on backend mode.

## Consequences

The interface between build planning and lowering carries more domain meaning than a raw compiler path, which improves locality for ABI and CRT changes. Future GNU-like Windows support should refine toolchain policy and resolution instead of adding another backend mode. Follow-up work may split payload form and executable realization further if more native backends make the current enum shallow. Invocation-scoped native contract selection follows the same toolchain-owned boundary and is recorded separately in [ADR 0003](0003-one-native-build-contract-per-invocation.md).
