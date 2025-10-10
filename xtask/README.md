## Sync commands docs to moonbit-docs
```
$ cargo xtask sync-docs --moonbit-docs-dir <path-to-moonbit-docs>
```

## RR parity helper
Run the Rupes Recta parity check and optionally rerun RR tests multiple times to flag unstable failures:

```
$ cargo xtask test-rr-parity --compare-baseline xtask/rr_expected_failures.txt --rr-runs 5
```

Passing `--rr-runs` greater than 1 highlights flaky RR-only tests separately while still treating them as failures.
