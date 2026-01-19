# Guideline for coding agents on MoonBuild

## Setting up MoonBit toolchain

You need an existing MoonBit toolchain installed to develop this project.
The preferred channel is currently `nightly`.

To check the presence of the toolchain, run `moon version` or `moonc -v`.

If you're working in a local environment, the environment is likely already set up by the owner.
If working in CI or a generic developing environment, you'll need to install it via the command provided in <https://www.moonbitlang.com/download/>.

At the time of writing, the commands are:

- Linux or MacOS: `curl -fsSL https://cli.moonbitlang.com/install/unix.sh | bash -s nightly`.
- Windows: `$env:MOONBIT_INSTALL_VERSION="nightly"; Set-ExecutionPolicy RemoteSigned -Scope CurrentUser; irm https://cli.moonbitlang.com/install/powershell.ps1 | iex`

If you are working from mainland China, using the `.cn` domain may result in a speedup.

## Other tools

- The latest stable Rust toolchain is required.
- To insert license headers, you can run `cargo {install, binstall -y} hawkeye` and then `hawkeye format`.
- Some maintainers use `jujutsu` to manage the repository. Check the local environment before choosing the tool to commit.

## Project layout

- `crates/`
  - `moon/`: The entry point to the `moon` utility.
    - `src/rr_build/`: Integration with Rupes Recta build engine.
    - `tests/test_cases/`: Integration snapshot tests, the majority of tests.
  - `moonbuild-rupes-recta/`: The new build graph generation engine (**now default**).
  - `moonbuild/`: The legacy build graph generation engine. Set `NEW_MOON=0` to use it if you encounter issues with Rupes Recta.
  - `moonbuild-debug/`: Debugging utilities, mainly around dry-run printing and snapshotting.
  - `mooncake/`: Library to resolve and download dependencies.
  - `moonrun/`: The runtime of WASM MoonBit programs.
  - `moonutil/`: Misc utilities including feature flags (`src/features.rs`).
- `docs/`: Documentation site
  - `manual/`, `manual-zh/`: User-facing documentation
  - `dev/`: Developer-facing documentation
    - `reference/`: Reference behaviors and models of the build system. Keep in sync with code.
- `xtask/`: Development utilities

### Build system transition

The project has transitioned from the legacy `moonbuild` to the new `moonbuild-rupes-recta` ("Rupes Recta") build engine:

- **Rupes Recta is now enabled by default**
- To use the legacy build system: set `NEW_MOON=0`
- Unstable features can be enabled via `-Z` flags (e.g., `-Z rr_export_module_graph`)
- See `crates/moonutil/src/features.rs` for all feature flags

## Coding guidelines

### Generic rules

- **Avoid hacks.** Only use one if there is truly no sane alternative, and then isolate and document it clearly. Never depend on properties coming from other modules that are not explicitly written down.
- Respect separation of concerns. Keep modules and functions single-purpose; prefer multiple explicit APIs or higher-order helpers over a single mode-switched function that multiplexes unrelated behaviors. When a complex control flow is shared but behavior differs drastically, factor out the common scaffolding and let callers inject the varying part via callbacks instead of encoding specific modes inside the function.
- Consistency is critical for PL tooling. Don’t add a feature to only a subset of commands when it's applicable and easy to support in all of them.
- Follow DRY. Don't leave three or more instances of the same logic around without a strong reason. If you see two copies and it's straightforward to merge them, do it.
- Don’t hesitate to refactor APIs. Prefer adding explicit fields and/or arguments over relying on implicit properties or hidden coupling in existing code.
- If a refactor significantly changes an API's mental model, present your plan for review before implementing it. If no maintainer is available, keep the refactor isolated and report it clearly afterwards.
- Prefer small, fine-grained, self-contained commits. Each commit should compile (and pass tests where applicable) on its own. When adding a regression test, first add a failing test that captures the bug (or rebase it before the fix), then apply the fix.
- Follow the "boy scout" rule. Leave every module you touch a bit cleaner than you found it.

### Tooling

- To check: `cargo clippy`. CI requires clippy to emit no warnings or errors to pass.
- To test: `cargo test --workspace`. CI requires all tests to pass. Some tests require the Git submodule to be set up to pass.
- To format: `cargo fmt`. CI requires no format errors. MoonBit code here are explicitly not required to be formatted (some tests even mandate that).
- To log: use `tracing`. Instrument potentially time-consuming functions.
- Errors: use `thiserror` in concrete APIs with a fixed error scheme, and `anyhow` on flexible, higher-level ones.
- Writing tests: most tests use `expect_test`, but `snapbox` is used when matching output patterns. Snapshot short outputs using `expect_test::expect![]`. Long outputs, especially command dry-runs, should go to `expect_test::expect_file![]`. Prefer graph comparison via `moon(test)::build_graph::compare_graphs` when snapshotting dry-run outputs. Only consider updating snapshots via `UPDATE_EXPECT=1` when you have changed behavior (e.g. commandline arguments).
