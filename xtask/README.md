## Checks

```bash
cargo xtask
```

This runs:

- `cargo run --bin moon -- check --manifest-path crates/moonbuild/template/test_driver_project/moon.mod.json`
- `cargo run --bin moon -- fmt --check --manifest-path crates/moonbuild/template/test_driver_project/moon.mod.json`
- `cargo fmt -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`

If any check fails, `xtask` prints plain copy-paste commands, for example:

- `cargo run --bin moon -- fmt --manifest-path crates/moonbuild/template/test_driver_project/moon.mod.json`
- `cargo clippy --fix --all-targets --all-features --allow-dirty --allow-staged`
- `cargo fmt`

`xtask ci` does not fail fast; it runs all checks and reports all failures at the end.

## Sync commands docs to moonbit-docs

```bash
cargo xtask sync-docs --moonbit-docs-dir <path-to-moonbit-docs>
```
