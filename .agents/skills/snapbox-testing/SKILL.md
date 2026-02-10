---
name: snapbox-testing
description: Write and maintain Rust CLI/integration tests with snapbox 0.6+ using command assertions, inline snapshots, and minimal wildcard/redaction patterns. Use when replacing manual `get_output` parsing, stabilizing cross-platform snapshots, or refining existing snapbox assertions.
---

# Snapbox Testing

Use this skill to make Rust command-output tests simpler, more stable, and easier to review.

## Follow This Workflow

1. Replace manual output plumbing with direct snapbox assertions.
2. Start from empty inline snapshots (`snapbox::str![""]`) for unstable/unknown output.
3. Run tests with `SNAPSHOTS=overwrite` to capture real output.
4. Tighten snapshots by keeping stable text exact and wildcarding only volatile parts.
5. Re-run tests and clippy.

Use:

```bash
SNAPSHOTS=overwrite cargo test -p <crate> --test <test-target>
```

## Prefer These Patterns

- Assert command result directly:
  - `.assert().success().stdout_eq(...)`
  - `.assert().failure().stderr_eq(...)`
- Prefer inline snapshots:
  - `snapbox::str![[r#"...\n"#]]` for multi-line outputs
  - string literals for short exact lines
- Keep assertions local and readable:
  - inline expected value when used once
  - avoid temporary `expected_*` bindings unless reused

## Minimize Redaction

- Prefer built-in wildcard filters first:
  - `[..]` for variable substrings within a line
  - `...` on its own line for variable trailing frames/lines
- Keep stable semantics visible:
  - preserve meaningful suffixes like `/_build/.../main.wasm`
  - avoid over-redacting fixed tokens (for example, exact error kind/message)
- Add custom `Redactions` only when wildcard patterns cannot express the instability clearly.

## Cross-Platform Guidance

- Rely on snapbox default path normalization before adding platform-specific `cfg` snapshots.
- Use one snapshot when separator normalization is sufficient.
- Add `cfg(windows)`/`cfg(not(windows))` split only for truly semantic OS differences.

## Avoid

- Manual `.get_output().stdout/stderr` + UTF-8 parsing when direct assertions work.
- Hand-written normalization helpers for paths/stack traces when snapbox patterns can express intent.
- Broad wildcarding that hides behavior regressions.

## Quick Checklist

- `cargo fmt --all`
- `cargo test -p <crate> --test <target>`
- `cargo clippy -p <crate> --tests -- -D warnings`
