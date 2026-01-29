This directory contains a small MoonBit project that defines the `moon test` driver templates.

Design goals:
- Keep templates compilable as a MoonBit project (catch drift early).
- Keep templates feature-based: Rust includes only the `template_*.mbt` files that correspond
  to tests actually present in `MooncGenTestInfo` (no-args / with-args / async / bench).
- Keep the generated driver backend-compatible via `#cfg` blocks for `native`, `js`, `wasm`, and `wasm-gc`.

How it is used:
- Rust assembles the final generated driver by `include_str!`ing:
  - `types.mbt`, `common.mbt`, `entry.mbt` (always)
  - a subset of `template_*.mbt` depending on which tests exist
- `fake.mbt` is never included (it only exists to demonstrate non-included files in this example project).
