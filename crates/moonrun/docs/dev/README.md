# Contributing Quick Start

## Developer workflows

- [Porting `moonbitlang/async` changes](async-upstream-porting.md): audit the
  one-way upstream runtime-port request queue, preserve exact provenance,
  deliver one upstream port at a time, and mark the originating async pull
  request completed after the Moon change lands.

## Proposed designs and research

- [Operating-system support for moonrun virtualization](os-virtualization-research.md):
  identify which isolation and lifecycle primitives can be delegated to Linux,
  macOS, and Windows and which semantics must remain inside moonrun.
- [Scheduler control-flow models for moonrun](scheduler-control-flow-research.md):
  compare Kubernetes, k0s, Nomad, and Wrangler ownership to separate
  declarative workload reconciliation from imperative virtual children.
- [Multi-service Runs and virtual child processes](multi-service-virtual-child-design.md):
  deepen moonrun around Engine with Deployment Controller, Execution
  Supervisor, and Host Process for transparent in-process Moonx children
  through incremental MVP slices.

## How to Build and Test

```bash
cargo build
cargo test
```

## Before PR

We encourage to add the following prefix to your commit message and PR title: feat, fix, internal, or minor.

It's recommended to run the following command before you submit a PR, which may help discover some potential ci failure ASAP

```bash
cargo fmt

cargo clippy --all-targets --all-features -- -D warnings

cargo test
```

We use [typos](https://github.com/crate-ci/typos) to avoid potential typos, you can also download and run it locally before PR.
