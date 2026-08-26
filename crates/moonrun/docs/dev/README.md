# Contributing Quick Start

## Developer workflows

- [Porting `moonbitlang/async` changes](async-upstream-porting.md): audit the
  one-way upstream runtime-port request queue, preserve exact provenance,
  deliver one upstream port at a time, and mark the originating async pull
  request completed after the Moon change lands.
- [Evolving Runtime isolation](runtime-isolation.md): preserve Ambient
  compatibility, keep authorization independent from Runtime behavior, audit
  process-global effects, and add isolated Host-domain adapters incrementally.

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
