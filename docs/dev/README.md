# Contributing Quick Start

## Contributor Copyright Assignment Agreement

Before submitting your changes to the moon project, you must sign Contributor Copyright Assignment Agreement (CCAA), depending on who owns the copyright to your work:
- For individual contributors, you can sign it online by visiting https://moonbitlang.com/cla/moon.
- For corporate contributors, please download the CCAA from [MoonBit_Build_System_Contributor_Copyright_Assignment_Agreement_V1.0.pdf](https://github.com/moonbitlang/moon/blob/CCAA/MoonBit_Build_System_Contributor_Copyright_Assignment_Agreement_V1.0.pdf), sign it, and send it to us at jichuruanjian@idea.edu.cn.

## Setup

The first thing is to install Rust toolchain and MoonBit toolchain, if you have not setup, referring to:

- [Install Rust](https://www.rust-lang.org/tools/install)
- [MoonBit CLI Tools](https://www.moonbitlang.com/download/#moonbit-cli-tools)

## Before changing Moon

For changes to MoonBuild concepts, command communication, package execution, or
native builds, start with the project [glossary](../../CONTEXT.md). Then use the
table below to load only the documents relevant to the work.

Read the affected source and tests as the authority for behavior already
implemented. Reference documents describe implemented behavior and must stay in
sync with it. Design and migration documents may also describe intended future
behavior.

| When changing | Read before design or review |
| --- | --- |
| Project discovery, modules, packages, dependency resolution, or internal packages | [Architecture and overview](reference/arch.md), [modules and packages](reference/modules-packages.md), then the relevant source and tests |
| Workspaces, package selection, `MOON_WORK`, or `MOON_NO_WORKSPACE` | [Workspace design](reference/workspace.md) and [architecture and overview](reference/arch.md) |
| Build planning, lowering, artifacts, n2 commands, or Rupes Recta behavior | [Architecture and overview](reference/arch.md), [package build process](reference/build.md), [build plan artifact dependencies](reference/build-plan-artifact-dependencies.md), and [Rupes Recta special cases](reference/rr-special-cases.md) |
| Compiler commands, target backends, conditional compilation, supported targets, or virtual packages | [Compiler commands](reference/compiler-cmd-ref.md), [supported targets](reference/supported-targets.md), [conditional compilation](reference/cond-comp.md), and [virtual packages](reference/virtual-pkg.md) as applicable |
| Native compilation, C stubs, toolchain selection, or Windows ABI/CRT policy | [Native ABI policy ADR](../adr/0002-native-abi-policy-belongs-to-toolchains.md), [native C toolchain resolution](reference/native-c-toolchain-resolution.md), and [toolchain layout](reference/toolchain-layout.md) |
| `moon test`, test-driver events, test filtering, snapshots, or choosing a test level | [Test suite strategy](reference/testing-strategy.md) and [`moon test` execution flow](reference/tests.md); use the repository-local [`snapbox-testing` skill](../../.agents/skills/snapbox-testing/SKILL.md) for snapbox assertions |
| Command stdout/stderr, logs, progress, child output, tracing lifecycle, delegation, or dry-run output | The glossary's [Command Communication](../../CONTEXT.md#command-communication), [command-results ADR](../adr/0004-separate-command-results-from-user-logs.md), [command output migration](command-output-migration.md), [CLI execution lifecycle](design/cli-execution-lifecycle.md), and [dry-run behavior](reference/dry-run.md) |
| `moon run` process launch, stdin, signals, temporary cleanup, or Windows Job Objects | [`moon run` process lifecycle](design/moon-run-process-lifecycle.md). This is the `moon` CLI process layer, not the wasm runtime implementation. |
| Moon home paths, global build cache, artifact identity, cleaning, or cross-compilation cache constraints | [Moon home layout](reference/moon-home-layout.md), [global build state and cache design](design/global-build-cache.md), and [build plan artifact dependencies](reference/build-plan-artifact-dependencies.md) |
| `moonx`, `moon runwasm`, executable package coordinates, binary installation, or binary discovery | [moonx dispatch ADR](../adr/0003-dispatch-moonx-by-executable-name.md), [`moonx`](reference/moonx.md), [`moon runwasm`](reference/runwasm.md), [`moon install`](reference/moon-install-binary.md), and [binary discovery](reference/binaries.md) as applicable |
| Prebuild tasks, bundle, indirect dependencies, or toolchain packaging | [Prebuild tasks](reference/prebuild.md), [`moon bundle`](reference/bundle.md), [indirect dependencies](reference/indirect-dep.md), or [toolchain layout](reference/toolchain-layout.md) as applicable |
| Wasm runtime imports, Handles, async host behavior, V8, or Wasmtime | Repository [async wasm host boundary ADR](../adr/0001-async-wasm-host-boundary.md), then the [`moonrun` developer documentation](../../crates/moonrun/docs/dev/README.md) |

If a proposed change contradicts an ADR or an implemented-behavior reference,
call that out explicitly instead of silently replacing it. The default build
engine is Rupes Recta; inspect legacy `moonbuild` behavior only when the changed
path still uses it or compatibility is part of the requirement.

## Document roles

- [`CONTEXT.md`](../../CONTEXT.md) owns project vocabulary. It is not an
  implementation specification.
- [`docs/adr`](../adr/) owns accepted, hard-to-reverse decisions and their
  trade-offs.
- [`docs/dev/design`](design/) owns cross-cutting designs, invariants, and
  reconsideration criteria.
- [`docs/dev/reference`](reference/readme.md) owns descriptions of implemented
  MoonBuild behavior and must change with that behavior.
- [`docs/dev/research`](research/) contains non-normative investigations that
  inform design choices but do not define implemented behavior, including the
  [dependency-tree deduplication comparison](research/dependency-tree-dedup-comparison.md).
- [Command output migration](command-output-migration.md) owns the sequencing
  of that in-progress migration.
- [`docs/manual`](../manual/) and [`docs/manual-zh`](../manual-zh/) own
  user-facing command documentation.
- Source and tests remain the final evidence for what the current revision
  actually implements.

The repository-local [`moon-development` skill](../../.agents/skills/moon-development/SKILL.md)
teaches coding agents how to use this index without copying its routing table.

## Decision index

- [0001: Async Wasm Host Boundary](../adr/0001-async-wasm-host-boundary.md)
- [0002: Native ABI Policy Belongs to Toolchains](../adr/0002-native-abi-policy-belongs-to-toolchains.md)
- [0003: Dispatch Moonx By Executable Name](../adr/0003-dispatch-moonx-by-executable-name.md)
- [0004: Separate Command Results from User Logs](../adr/0004-separate-command-results-from-user-logs.md)
- [0005: Prepare Standalone Dependencies Before Script Execution](../adr/0005-plan-standalone-dependencies-separately.md)
- [0006: Model Providers as Build Graph Topology](../adr/0006-model-providers-as-build-graph-topology.md)

## How to Build and Test

### On Unix

```bash
cargo xtask
cargo build
cargo test
```

`cargo xtask` runs auxiliary checks (`moon check`, `moon fmt --check` for the test driver template, `cargo fmt -- --check`, and `cargo clippy --all-targets --all-features -- -D warnings`). If checks fail, it prints copy-paste fix commands.

The tests for `moon` depend on
[expect-test](https://github.com/rust-analyzer/expect-test). If your changes
affect the output of the tests, you can update the expected output:

```bash
env UPDATE_EXPECT=1 cargo test
```

### On Windows

```bash
cargo xtask
cargo build
cargo test
```

```powershell
$env:UPDATE_EXPECT=1; cargo test; Remove-Item Env:\UPDATE_EXPECT
```

## How to Install

The following command will install `moon` from source code, it will be installed at `~/.cargo/bin/`.(note that the original `moon` install via [MoonBit CLI Tools](https://www.moonbitlang.com/download/#moonbit-cli-tools) is at `~/.moon/bin/`)

### Release Install

```bash
cargo install --path ./crates/moon
```

### Debug Install (faster)

```bash
cargo install --path ./crates/moon --debug
```

```bash
# more faster
cargo install --path ./crates/moon --debug --offline
```


## Source Code Overview

### Design

- [Moon CLI execution lifecycle](design/cli-execution-lifecycle.md)
- [Global build state and cache design](design/global-build-cache.md)
- [`moon run` process lifecycle](design/moon-run-process-lifecycle.md)
- [`docs/dev/reference`](reference/readme.md): implemented MoonBuild behavior.

- `crates/moon`
  - `src/cli`: the command line interface of `moon`
    - `src/cli/mooncake_adapter.rs`: forwards to the `mooncake`
      binary
    - `src/cli/generate_test_driver.rs`: as the name suggests
  - `src/rr_build`: integration with the Rupes Recta build engine
  - `tests/test_cases`: end-to-end tests organized into modules by purpose;
    `mod.rs` contains their shared imports and module registrations

- `crates/moonbuild-rupes-recta`: the new build graph generation engine (now default)
  - `src/build_lower`: lowers resolved modules to n2 build commands
  - `src/fmt.rs`: formatting support
  - `src/metadata.rs`: metadata generation for IDE/tooling
  - See `docs/dev/reference/compiler-cmd-ref.md` for compiler command reference

- `crates/moonbuild`: the legacy build graph generation engine
  - Being phased out in favor of `moonbuild-rupes-recta`
  - `src/{check, gen, build, bundle, entry, runtest}`: generate
    commands and n2 state according to `moon.mod.json` and `moon.pkg.json`
  - `src/bundle.rs`: only for `moonbitlang/core`, not visible
    to users
  - `src/dry_run.rs`: prints commands without executing them,
    mainly used by end-to-end tests.
  - `src/expect.rs`: the implementation of expect tests in
    `moon`

- `crates/mooncake`: package manager
  - `src/pkg/add`: `moon add`
  - `src/pkg/{install, sync}`: `moon install`
  - `src/pkg/remove`: `moon remove`
  - `src/pkg/tree`: `moon tree`
  - `src/registry/client.rs`: synchronizes registry metadata and downloads
    verified packages and prebuilt wasm assets
  - `src/resolver/mvs.rs`: Go-like minimal version selection
    algorithm.

- `crates/moonutil`: shared utilities
  - `src/common.rs`: common definitions shared by other crates
  - `src/scan.rs`: scans the project directory to gather all
    structural information
  - `src/moon_dir.rs`: owns paths and environment selection for mutable
    `MOON_HOME` state and the installed toolchain
  - `src/features.rs`: unstable feature flags (`rr_*`)
  - `src/build.rs`: for `moon version`

- `crates/moonrun`: runtime for executing WASM MoonBit programs

- `crates/moonbuild-debug`: debugging utilities for dry-run printing and snapshotting

## Before PR

It's recommended to run the following command before you submit a PR, which may
help discover some potential CI failure ASAP

```bash
cargo xtask
cargo test
```

We use [typos](https://github.com/crate-ci/typos) to avoid potential typos, you
can also download and run it locally before PR.


## Before Merging
### Maintain Semi-Linear History

To keep a clean and readable Git history, we follow a semi-linear history pattern. A semi-linear history looks like this:

```
$ git log --oneline --graph
*
|\
| *
|/
*
|\
| *
| *
|/
*
```

A semi-linear history improves readability, simplifies bug tracking.

Until GitHub supports this natively (see discussion: [Support semi-linear history](https://github.com/orgs/community/discussions/8940)), we use rebase workflow and create a merge commit when merging a pull request to achieve a semi-linear history.

### Updating Branches

There are two ways to update branches: locally and on the GitHub Pull Request page.

#### Updating Branches Locally

1. Fetch the latest changes:
    ```
    git fetch
    ```
2. Rebase your branch:
    ```
    git rebase origin/main
    ```

#### Updating Branches on GitHub

When updating a branch on the GitHub Pull Request page, always use the **"Update with Rebase"** option instead of "Update with merge commit." This helps in maintaining the desired semi-linear history.
