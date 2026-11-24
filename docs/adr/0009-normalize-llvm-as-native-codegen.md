---
status: proposed
---

# Normalize LLVM as Native Code Generation

Native target requirements and the implementation used to generate code are
separate choices. Model LLVM as an explicit Native code-generation request.
Keeping LLVM as a peer backend duplicates platform and resource configuration;
separating the choices lets Native share those settings while each generator
retains its capability and toolchain constraints.

Planning honors an explicit code-generation request instead of silently falling
back to another implementation. Automatic selection still follows the toolchain
precedence established by [ADR 0002](0002-native-abi-policy-belongs-to-toolchains.md).
Sharing a Native family does not imply interchangeable artifacts or toolchains;
ABI and CRT policy remain owned by that decision.

The trade-off is a compatibility boundary: existing interfaces distinguish
`native` and `llvm`, so they need a derived selector until their contracts are
revised. Normalizing the internal request does not itself change those contracts.
