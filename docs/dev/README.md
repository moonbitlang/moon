# MoonBuild developer guide

Start with the task you are doing. Read the linked section, then follow its
source and test pointers; the documents below are not a required reading list.

| Task | Start here |
| --- | --- |
| Set up, build, or submit a change | [Contributing](contributing.md) |
| Find the crate that owns a behavior | [Source map](contributing.md#source-map) |
| Write or clean up a test | [Testing](testing.md) |
| Measure fixture overhead or retain a test workspace | [Test-directory diagnostics](test-directory-diagnostics.md) |
| Look up a Moon command as a user | [English manual](../manual/src/SUMMARY.md) or [Chinese manual](../manual-zh/src/SUMMARY.md) |
| Change MoonBuild behavior | The [glossary](../../CONTEXT.md), then the relevant route below |

## Find the relevant behavior

Source and tests establish current behavior. References describe that behavior;
designs and migrations may describe work still to do. Call out conflicts with
accepted decisions or references when proposing a change.

Rupes Recta is the default build engine. Inspect legacy `moonbuild` paths when
they own the behavior being changed or compatibility is part of the task.

| When changing | Start here | Follow when needed |
| --- | --- | --- |
| Project discovery, modules, packages, imports | [Modules and packages](reference/modules-packages.md) | Architecture: [discovery](reference/arch.md#project-discovery-and-layout), [module resolution](reference/arch.md#module-dependency-management), [package resolution](reference/arch.md#package-and-package-dependency-management) |
| Workspaces or package selection | [Workspace behavior](reference/workspace.md) | [Directory and environment facts](reference/arch.md#directory-and-environment-facts) |
| Planning, lowering, artifacts, or execution | [Pipeline overview](reference/arch.md#the-pipeline) | [Package build process](reference/build.md), [artifact dependencies](reference/build-plan-artifact-dependencies.md), [RR special cases](reference/rr-special-cases.md) |
| Compiler arguments or backend support | [Compiler commands](reference/compiler-cmd-ref.md) | [Supported targets](reference/supported-targets.md), [conditional compilation](reference/cond-comp.md), [virtual packages](reference/virtual-pkg.md) |
| Native builds, C stubs, ABI, or CRT | [Native ABI decision](../adr/0002-native-abi-policy-belongs-to-toolchains.md) | [C toolchain resolution](reference/native-c-toolchain-resolution.md), [toolchain layout](reference/toolchain-layout.md) |
| `moon test`, filtering, driver events, or promotion | [`moon test` execution flow](reference/tests.md) | [Writing tests for Moon](testing.md) |
| stdout/stderr, logs, or progress | [Command communication vocabulary](../../CONTEXT.md#command-communication) and [output decision](../adr/0004-separate-command-results-from-user-logs.md) | [CLI lifecycle](design/cli-execution-lifecycle.md), [output migration](command-output-migration.md) |
| Dry-run output | [Dry-run behavior](reference/dry-run.md) | [CLI lifecycle](design/cli-execution-lifecycle.md) |
| `moon run` process launch, stdin, signals, or cleanup | [`moon run` process lifecycle](design/moon-run-process-lifecycle.md) | This is the CLI process layer; runtime internals use the Moonrun route below. |
| Moon home, global caches, or cleaning | [Moon home layout](reference/moon-home-layout.md) | [Cache design](design/global-build-cache.md), [artifact dependencies](reference/build-plan-artifact-dependencies.md) |
| Registry package execution or installed binaries | [`moonx`](reference/moonx.md) or [`moon install`](reference/moon-install-binary.md) | [moonx dispatch decision](../adr/0003-dispatch-moonx-by-executable-name.md), [binary discovery](reference/binaries.md) |
| Prebuild tasks, bundles, or indirect dependencies | [Prebuild](reference/prebuild.md), [bundle](reference/bundle.md), or [indirect dependencies](reference/indirect-dep.md) | [Toolchain layout](reference/toolchain-layout.md) for packaging |
| Wasm runtime imports, Handles, async hosts, V8, or Wasmtime | [Async host boundary decision](../adr/0001-async-wasm-host-boundary.md) | [Moonrun developer guide](../../crates/moonrun/docs/dev/README.md) |

## Keep each fact in one place

- [Contributing](contributing.md) and [Testing](testing.md) own contributor
  procedures.
- [`CONTEXT.md`](../../CONTEXT.md) owns project vocabulary. It is not an
  implementation specification.
- [`docs/adr`](../adr/) owns accepted, hard-to-reverse decisions and their
  trade-offs.
- [`docs/dev/design`](design/) owns cross-cutting designs, invariants, and
  reconsideration criteria.
- [`docs/dev/reference`](reference/readme.md) owns descriptions of implemented
  MoonBuild behavior and must change with that behavior.
- [`docs/dev/research`](research/) contains non-normative investigations that
  inform design choices but do not define implemented behavior. Existing
  comparisons include [dependency-tree deduplication](research/dependency-tree-dedup-comparison.md)
  and [GitHub permalink installation](research/github-permalink-install.md).
- [Command output migration](command-output-migration.md) owns the sequencing
  of that in-progress migration.
- [`docs/manual`](../manual/) and [`docs/manual-zh`](../manual-zh/) own
  user-facing command documentation.
- Source and tests remain the final evidence for what the current revision
  actually implements.

When editing a page, keep its purpose explicit and link to facts owned elsewhere.
Remove obsolete instructions after checking their source; Git retains the old
text. Keep temporary audits and proposals out of behavior references. Update
this index when a page moves or its audience changes.

The repository-local [`moon-development` skill](../../.agents/skills/moon-development/SKILL.md)
teaches coding agents how to use this index without copying its routing table.

## Decision index

- [0001: Async Wasm Host Boundary](../adr/0001-async-wasm-host-boundary.md)
- [0002: Native ABI Policy Belongs to Toolchains](../adr/0002-native-abi-policy-belongs-to-toolchains.md)
- [0003: Dispatch Moonx By Executable Name](../adr/0003-dispatch-moonx-by-executable-name.md)
- [0004: Separate Command Results from User Logs](../adr/0004-separate-command-results-from-user-logs.md)
- [0005: Use Unified Planning and Execution for Standalone Builds](../adr/0005-plan-standalone-dependencies-separately.md)
- [0006: Model Providers as Build Graph Topology](../adr/0006-model-providers-as-build-graph-topology.md)
