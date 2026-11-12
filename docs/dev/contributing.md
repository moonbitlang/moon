# Contributing to MoonBuild

Use this page to build and submit a change. For behavior-specific reading, use
the [developer index](README.md); for assertions and test boundaries, use
[Testing](testing.md).

## Set up and build

Install [Rust](https://www.rust-lang.org/tools/install) and the
[MoonBit toolchain](https://www.moonbitlang.com/download/). The repository's
[agent guidelines](../../AGENTS.md) specify the preferred toolchain channel.
Initialize the pinned submodules before development or updating snapshots:

```sh
git submodule update --init --recursive
cargo build
```

For an installed build, run `cargo install --path crates/moon`; add `--debug`
for a faster development build. Cargo installs into its configured binary
directory, which may differ from the MoonBit toolchain's binary directory.
Check which `moon` your shell resolves before testing an installed build.

## Source map

| Area | Entry points |
| --- | --- |
| CLI parsing and command orchestration | [`moon/src/cli`](../../crates/moon/src/cli), [`cli.rs`](../../crates/moon/src/cli.rs) |
| CLI integration with Rupes Recta and dry-run rendering | [`moon/src/rr_build`](../../crates/moon/src/rr_build) |
| Discovery, resolution, planning, and lowering | [`moonbuild-rupes-recta/src`](../../crates/moonbuild-rupes-recta/src/lib.rs) |
| Execution Plans, n2 scheduling, and diagnostics | [`moonbuild/src/execution`](../../crates/moonbuild/src/execution) |
| Test results and expect/snapshot promotion | [`moonbuild/src/runtest.rs`](../../crates/moonbuild/src/runtest.rs), [`expect.rs`](../../crates/moonbuild/src/expect.rs) |
| Package management and registry access | [`mooncake/src/pkg`](../../crates/mooncake/src/pkg), [`resolver`](../../crates/mooncake/src/resolver), [`registry`](../../crates/mooncake/src/registry) |
| Paths, feature flags, and shared types | [`moonutil/src`](../../crates/moonutil/src) |
| Wasm runtime | [Moonrun developer guide](../../crates/moonrun/docs/dev/README.md) |
| Tests and their support code | [Test boundaries and examples](testing.md#choose-the-test-level) |

## Validate a change

Start with the affected test using the commands in [Testing](testing.md#run-and-review).
Before submitting, run the repository checks:

```sh
cargo xtask
cargo test --workspace
```

On Linux, the profiling E2E tests require `perf` with DWARF unwinding support
and permission to record per-process `cpu-clock` events. The existing Linux CI
test job installs the kernel tools and sets `kernel.perf_event_paranoid=1`.

`cargo xtask` checks the test-driver template with Moon, Rust formatting, and
Clippy, including the runtime backend configurations. The exact commands live
in [`xtask/src/ci.rs`](../../xtask/src/ci.rs). CI also checks spelling with
[typos](https://github.com/crate-ci/typos); see the
[CI workflow](../../.github/workflows/ci.yml) for platform coverage.

Only update expected output for an intentional behavior change or a reviewed
test migration. [Snapshot review](testing.md#run-and-review) describes how to
target an update without rewriting unrelated expectations.

## Submit and merge

Before submitting, sign the Contributor Copyright Assignment Agreement (CCAA):

- Individuals: [sign online](https://moonbitlang.com/cla/moon).
- Corporations: download the
  [CCAA](https://github.com/moonbitlang/moon/blob/CCAA/MoonBit_Build_System_Contributor_Copyright_Assignment_Agreement_V1.0.pdf),
  sign it, and send it to jichuruanjian@idea.edu.cn.

Follow the [repository guidelines](../../AGENTS.md) and
[PR template](../../.github/PULL_REQUEST_TEMPLATE.md). Explain the problem,
why the change is correct, and the boundary of the change.

Keep semi-linear history: rebase the branch onto the latest `origin/main`, then
merge with a merge commit. On GitHub, choose **Update with Rebase** when updating
a PR branch. This preserves each PR as a contiguous sequence without merge
commits from branch updates.
