# Preserve Native-Shaped Worker Boundary

Moonrun will preserve the `moonbitlang/async` native-shaped `Worker` and `Job` boundary for wasm instead of exposing a separate wasm-specific scheduler API to MoonBit. In wasm, worker handles may represent scheduler resources rather than raw OS threads, but the MoonBit-facing structure should stay close to the native async implementation to reduce maintenance drift.

When checking parity with native behavior, evaluate the native C stubs together
with the MoonBit code that wraps them. The compatibility target is the behavior
reachable through the `moonbitlang/async` MoonBit API, not every possible misuse
of a raw C symbol. If the MoonBit layer constrains ownership or lifecycle, such
as creating a private `Job`, submitting it once, reading the result, and freeing
it, the wasm host should model that contract explicitly and document any
deliberately stricter checks against raw C misuse.
