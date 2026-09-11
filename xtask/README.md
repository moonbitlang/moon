## Checks

```bash
cargo xtask
```

This runs:

- `cargo run --bin moon -- -C crates/moonbuild/template/test_driver_project check`
- `cargo run --bin moon -- -C crates/moonbuild/template/test_driver_project fmt --check`
- `cargo fmt -- --check`
- `cargo clippy --workspace --exclude moonrun --all-targets --all-features -- -D warnings`
- `cargo clippy -p moonrun --all-targets -- -D warnings`
- `cargo clippy -p moonrun --all-targets --no-default-features --features wasmtime -- -D warnings`

If any check fails, `xtask` prints plain copy-paste commands, for example:

- `cargo run --bin moon -- -C crates/moonbuild/template/test_driver_project fmt`
- `cargo clippy --fix --workspace --exclude moonrun --all-targets --all-features --allow-dirty --allow-staged`
- `cargo clippy --fix -p moonrun --all-targets --allow-dirty --allow-staged`
- `cargo clippy --fix -p moonrun --all-targets --no-default-features --features wasmtime --allow-dirty --allow-staged`
- `cargo fmt`

`xtask ci` does not fail fast; it runs all checks and reports all failures at the end.

## Sync commands docs to moonbit-docs

```bash
cargo xtask sync-docs --moonbit-docs-dir <path-to-moonbit-docs>
```
