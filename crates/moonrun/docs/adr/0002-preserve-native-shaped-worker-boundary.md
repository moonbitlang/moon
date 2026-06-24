# Preserve Native-Shaped Worker Boundary

Moonrun will preserve the `moonbitlang/async` native-shaped `Worker` and `Job` boundary for wasm instead of exposing a separate wasm-specific scheduler API to MoonBit. In wasm, worker handles may represent scheduler resources rather than raw OS threads, but the MoonBit-facing structure should stay close to the native async implementation to reduce maintenance drift.
