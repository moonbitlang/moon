# Contributing Quick Start

## Setup

The first thing is to install rust toolchain and moonbit toolchain, if you have not setup, referring to:

- [rust toolchain installation](https://www.rust-lang.org/tools/install)
- [moonbit toolchain installation](https://www.moonbitlang.com/download/#moonbit-cli-tools)


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

The following command will install `moon` from source code, it will be installed at `~/.cargo/bin/`.(note that the original `moon` install via [moonbit toolchain installation](https://www.moonbitlang.com/download/#moonbit-cli-tools) is at `~/.moon/bin/`)

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

The following content is based on [a59ebb84](https://github.com/moonbitlang/moon/commit/a59ebb8406caa91729a56f9e166cc160720e3dd0), which may outdated as the project develops.

- `crates/moon`
  - `src/cli`: the command line interface of `moon`
    - `src/cli/mooncake_adapter.rs`: forwards to the `mooncake`
      binary
    - `src/cli/generate_test_driver.rs`: as the name suggests
  - `tests/test_cases/mod.rs`: all end-to-end tests are located in
    this file

- `crates/moonbuild`
  - `src/{check, gen, build, bundle, entry, runtest}`: generate
    commands and n2 state according to `moon.mod.json` and `moon.pkg.json`
  - `src/bundle.rs`: only for `moonbitlang/core`, not visible
    to users
  - `src/bench.rs`: generates a project for benchmarking, will
    be moved to `moon new`
  - `src/dry_run.rs`: prints commands without executing them,
    mainly used by end-to-end tests.
  - `src/expect.rs`: the implementation of expect tests in
    `moon`
  - `src/upgrade.rs`: for `moon upgrade`. We hope to move it to
    another binary crate, not mooncake, since depending on network in moonbuild
    does not make sense.

- `crates/mooncake`: package manager
  - `src/pkg/add`: `moon add`
  - `src/pkg/{install, sync}`: `moon install`
  - `src/pkg/remove`: `moon remove`
  - `src/pkg/tree`: `moon tree`
  - `src/registry/online.rs`: downloads packages from
    mooncakes.io
  - `src/resolver/mvs.rs`: Go-like minimal version selection
    algorithm.

- `crates/moonutil`: currently not well organized, needs cleanup
  - `src/common.rs`: common definitions shared by other crates
  - `src/scan.rs`: scans the project directory to gather all
    structural information
  - `src/moon_dir.rs`: gets the `.moon`, `core`, etc. directory
    paths and handles related environment variables
  - `src/build.rs`: for `moon version`


## Before PR

It's recommended to run the following command before you submit a PR, which may
help discover some potential CI failure ASAP

```bash
cargo fmt

cargo clippy --all-targets --all-features -- -D warnings

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
