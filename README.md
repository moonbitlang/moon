[![codecov](https://codecov.io/github/moonbitlang/moon/graph/badge.svg?token=0Rzd0aDlCY)](https://codecov.io/github/moonbitlang/moon)

# moon

The build system and package manager for MoonBit.

## Current Status

🚧 Prepare for open source (expected at the end of June or early July).

## How to Build and Test

### On Unix

```bash
cargo build
cargo test
```

The tests for `moon` depend on
[expect-test](https://github.com/rust-analyzer/expect-test). If your changes
affect the output of the tests, you can update the expected output:

```bash
env UPDATE_EXPECT=1 cargo test
```

### On Windows

```bash
cargo build
cargo test
```

```powershell
$env:UPDATE_EXPECT=1; cargo test; Remove-Item Env:\UPDATE_EXPECT
```

## How to Install

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

## Before Contribute

It's recommended to run the following command before you submit a PR, which may
help discover some potential ci failure ASAP

```bash
cargo fmt

cargo clippy --all-targets --all-features -- -D warnings

cargo test
```

We use [typos](https://github.com/crate-ci/typos) to avoid potential typos, you
can also download and run it locally before PR.

## Source Code Overview

- `crates/moon`
  - `crates/moon/src/cli`: the command line interface of `moon`
    - `crates/moon/src/cli/mooncake_adapter.rs`: forwards to the `mooncake`
      binary
    - `crates/moon/src/cli/generate_test_driver.rs`: as the name suggests
  - `crates/moon/tests/test_cases/mod.rs`: all end-to-end tests are located in
    this file

- `crates/moonbuild`
  - `crates/moonbuild/src/{check, gen, build, bundle, entry, runtest}`: generate
    commands and n2 state according to `moon.mod.json` and `moon.pkg.json`
  - `crates/moonbuild/src/bundle.rs`: only for `moonbitlang/core`, not visible
    to users
  - `crates/moonbuild/src/bench.rs`: generates a project for benchmarking, will
    be moved to `moon new`
  - `crates/moonbuild/src/dry_run.rs`: prints commands without executing them,
    mainly used by end-to-end tests.
  - `crates/moonbuild/src/expect.rs`: the implementation of expect tests in
    `moon`
  - `crates/moonbuild/src/upgrade.rs`: for `moon upgrade`. We hope to move it to
    another binary crate, not mooncake, since depending on network in moonbuild
    does not make sense.

- `crates/mooncake`: package manager
  - `crates/mooncake/src/pkg/add`: `moon add`
  - `crates/mooncake/src/pkg/{install, sync}`: `moon install`
  - `crates/mooncake/src/pkg/remove`: `moon remove`
  - `crates/mooncake/src/pkg/tree`: `moon tree`
  - `crates/mooncake/src/registry/online.rs`: downloads packages from
    mooncakes.io
  - `crates/mooncake/src/resolver/mvs.rs`: Go-like minimal version selection
    algorithm.

- `crates/moonutil`: currently not well organized, needs cleanup
  - `crates/moonutil/src/common.rs`: common definitions shared by other crates
  - `crates/moonutil/src/scan.rs`: scans the project directory to gather all
    structural information
  - `crates/moonutil/src/moon_dir.rs`: gets the `.moon`, `core`, etc. directory
    paths and handles related environment variables
  - `crates/moonutil/src/build.rs`: for `moon version`
