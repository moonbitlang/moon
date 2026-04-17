# Workspace Design

This document describes Moon's current workspace model: manifest formats,
project selection, command categories, and the boundaries of what workspace
mode does and does not support.

The implementation lives mainly in:

- `crates/moonutil/src/dirs.rs`
- `crates/moonutil/src/workspace.rs`
- `crates/mooncake/src/pkg/mod.rs`
- `crates/mooncake/src/pkg/sync.rs`
- `crates/moon/src/cli/mooncake_adapter.rs`
- `crates/moonbuild-rupes-recta/src/discover/mod.rs`
- `crates/moonbuild-rupes-recta/src/fmt.rs`

## Goals

Workspace support is meant to provide one consistent model for:

1. selecting the effective project for a command
2. deciding whether that project is a single module or a workspace
3. deciding whether the command needs one selected member module or the whole
   selected project

The design intentionally avoids a separate enum like "project mode". The source
of truth is the selected manifest path itself.

## Manifest Model

Moon accepts one workspace manifest format:

- `moon.work`
  - the current DSL form

A workspace manifest defines:

- `members`
  - the module directories contained by the workspace
- `preferred_target`
  - an optional workspace-wide default backend

Workspace members are canonicalized relative to the workspace root and deduped.

The workspace root may or may not also contain a `moon.mod.json`.

## Default Backend Resolution

When a command needs a backend target, Moon resolves it in this order:

1. explicit `--target`
2. workspace `preferred_target`, if present
3. the selected module's `preferred-target`
4. `wasm-gc`

If `moon.work` omits `preferred_target`, different workspace members may still
resolve to different default backends.

## Selection Result

`SourceTargetDirs` resolves command input into:

- `project_manifest_path`
  - one of `moon.mod.json` or `moon.work`
- `project_root`
  - the parent directory of `project_manifest_path`
- `module_dir`
  - `Some(member_dir)` when a command selected a module inside a workspace
  - `None` when a command selected a workspace root directly

The intended interpretation is simple:

- `project_manifest_path = moon.mod.json`
  - single-module behavior
- `project_manifest_path = moon.work`
  - workspace behavior

`module_dir` is orthogonal:

- it does not decide whether workspace mode is enabled
- it only tells member-scoped commands which member they should act on

## Command Model

Workspace behavior is easier to understand if commands are split into three
categories.

The list below focuses on the current workspace-specific command split. Other
commands still use the same project-selection layer, but are less important to
the workspace model itself.

### Project-Scoped Commands

These commands operate on the selected project as a whole.

Representative examples:

- `moon build`
- `moon check`
- `moon test`
- `moon bench`
- `moon bundle`
- `moon fmt`
- `moon info`

When the selected manifest is a workspace manifest, these commands operate on
the whole workspace. They can be run from:

- the workspace root
- a workspace member directory
- a nested non-module directory under the workspace
- `--manifest-path <member>/moon.mod.json`
- `--manifest-path moon.work`

They do not need an implicit default member.

For project-scoped commands that consume a backend (`build`, `check`, `test`,
`bench`, `bundle`, `info`):

- with explicit `--target`, Moon keeps the existing single-backend behavior
- without `--target`, Moon uses the workspace default rules above
- if local modules resolve to different default backends, Moon runs one
  backend-specific subplan per group instead of warning and falling back to
  `wasm-gc`
- explicit package/path filters are grouped by the owning module in the same
  way

`moon test --update` still requires one backend and errors if the default
grouping would span multiple backends.

### Member-Scoped Commands

These commands need one concrete module even when the selected project is a
workspace.

Current examples:

- `moon run`
- `moon add`
- `moon remove`
- `moon tree`
- `moon package`
- `moon publish`
- `moon doc`
- `moon prove`

These commands are still workspace-aware:

- they keep workspace-local dependency resolution
- they keep workspace-local build layout
- they use the selected member as the operation target

But they are not workspace-wide commands.

When these commands omit `--target`, Moon uses the selected member module's
effective default backend.

At a workspace root, they fail unless Moon can determine a member module from
context or from the command input. In practice, that means:

- `moon run` can infer the member from its required package/path selector
- running them from a member directory is supported
- passing `--manifest-path <member>/moon.mod.json` is supported

This is why `run`, `publish`, `package`, `doc`, and `prove` only work for one
selected module at a time today. There is no "publish the whole workspace",
"generate docs for the whole workspace", or "run the whole workspace" mode in
the current design.

### Workspace Maintenance Commands

These commands manage the workspace manifest itself:

- `moon work init`
- `moon work use`
- `moon work sync`

Their model is different from normal project commands:

- `work init`
  - creates a `moon.work`
- `work use`
  - updates an existing applicable workspace if one already applies
  - otherwise stays local and creates/updates a workspace rooted at the current
    module or directory
- `work sync`
  - requires a workspace manifest
  - syncs workspace-local dependency versions into member manifests

`work sync` is workspace-only. It is not meaningful in plain single-module mode.

## Supported And Unsupported Behaviors

The current design supports:

- workspace roots that contain only `moon.work`
- workspace roots that also contain `moon.mod.json`
- selecting a member module from inside the member directory
- selecting a member module with `--manifest-path <member>/moon.mod.json`
- whole-workspace `build` / `check` / `test` / `bench` / `bundle` / `fmt` /
  `info`
- no-`--target` project-scoped execution in mixed-default workspaces by running
  one backend-specific subplan per effective default backend
- `run` selecting a workspace member from its package/path input while still
  using workspace-local dependency resolution
- member-scoped `package` / `publish` / `doc` / `prove` while still using
  workspace-local dependency resolution
- nested discovery from non-module directories inside a workspace

The current design does not support:

- an implicit default member at workspace root for member-scoped commands
- workspace-wide `publish`
- workspace-wide `package`
- workspace-wide `doc`
- workspace-wide `prove`
- workspace-wide `add` / `remove` / `tree`

Those commands need a selected member and will fail at workspace root with the
"cannot infer a target module in workspace" error.

## Selection Inputs

Project selection depends on:

- the working directory after `-C`
- `--manifest-path`
- `MOON_NO_WORKSPACE`

`--manifest-path` pins manifest resolution, but does not change the process
working directory.

## `MOON_NO_WORKSPACE`

`MOON_NO_WORKSPACE` is Moon's "workspace off" switch.

If it is set to a non-`0` value:

- implicit workspace discovery is disabled
- explicit `--manifest-path moon.work` is also disabled
- commands behave as if workspace mode does not exist

This is intentionally close to `GO_WORK=off` in Go.

The resulting behavior is:

- if a `moon.mod.json` can be found from the selected start directory upward,
  Moon uses that module
- otherwise the command behaves as "not in a Moon module"

This applies even when:

- `moon.work` and `moon.mod.json` are colocated at the same root
- `--manifest-path` explicitly points to a workspace manifest

So `MOON_NO_WORKSPACE` does not mean "disable only implicit promotion into a
workspace". It disables workspace behavior completely.

## Selection Without `--manifest-path`

Without `--manifest-path`, Moon starts from the current directory after `-C`.

The algorithm is:

1. Canonicalize the current directory.
2. If `MOON_NO_WORKSPACE` is enabled:
   - find the nearest ancestor containing `moon.mod.json`
   - if found, select that module manifest
   - otherwise, fail because workspace mode is disabled and no module exists
3. Otherwise, walk ancestors from nearest to farthest and look for applicable
   workspace manifests.
4. If an applicable workspace manifest is found, select that workspace
   manifest.
5. If no applicable workspace is found, fall back to the nearest ancestor
   `moon.mod.json`.
6. If neither exists, fail with "not in a Moon project".

## Selection With `--manifest-path`

`--manifest-path` accepts exactly:

- `moon.mod.json`
- `moon.work`

The selected manifest path is canonicalized first.

### `--manifest-path <...>/moon.mod.json`

This means "start from this module".

- if `MOON_NO_WORKSPACE` is enabled, the command stays in single-module mode
- otherwise, Moon may still promote to an enclosing applicable workspace

This is important: `--manifest-path moon.mod.json` does not mean "force
single-module mode". It means "select this module as the starting point".

If an enclosing workspace applies, the command keeps:

- `project_manifest_path = workspace manifest`
- `project_root = workspace root`
- `module_dir = selected module`

That is how member-scoped commands can act on one member while still using the
workspace's local dependency graph.

### `--manifest-path <...>/moon.work`

This means "start from this workspace root", but only when workspace mode is
enabled.

If `MOON_NO_WORKSPACE` is enabled:

- Moon ignores the workspace manifest
- Moon falls back to the nearest ancestor `moon.mod.json`, if any
- otherwise the command fails because workspace mode is disabled and no module
  is available

## How Moon Decides Whether A Workspace Applies

Workspace applicability is order-sensitive.

Moon walks ancestors from nearest to farthest. While walking, it derives the
current module boundary from the same ancestor order instead of precomputing one
outer module root before the walk.

This preserves the intended precedence:

- a nearer applicable workspace should win
- a farther workspace may still apply later if it explicitly lists the selected
  module as a member
- an unrelated outer `moon.mod.json` must not make Moon skip a nearer workspace
  that should still apply

This matters for layouts like:

```text
outer/
  moon.mod.json
  ws/
    moon.work
    app/
      moon.mod.json
```

From `outer/ws`, the nearer workspace should win while workspace mode is
enabled.

With `MOON_NO_WORKSPACE=1`, the workspace is ignored and selection falls back to
the nearest ancestor module, which is `outer/moon.mod.json`.

## Colocated `moon.work` And `moon.mod.json`

If a directory contains both:

- with workspace mode enabled, Moon may select the workspace manifest
- with `MOON_NO_WORKSPACE=1`, Moon must select `moon.mod.json` instead

This was the bug shape that motivated the recent cleanup of workspace
selection.

## Examples

| Start point / flags | Result |
| --- | --- |
| workspace root + `build` / `check` / `test` / `bench` / `bundle` / `fmt` / `info` | operate on the whole workspace; without `--target`, backend-consuming commands may run one plan per effective default backend |
| workspace root + `run <selector>` | select the member from the selector and use that member's effective default backend when `--target` is omitted |
| workspace root + `add` / `remove` / `tree` / `package` / `publish` / `doc` / `prove` | error: no target member can be inferred |
| member directory + member-scoped command | target that member and keep workspace context |
| `--manifest-path app/moon.mod.json` | start from `app`, then allow workspace promotion |
| `--manifest-path app/moon.mod.json` + member-scoped command | act on `app`, but keep workspace-local deps/layout if a workspace applies |
| `--manifest-path moon.work` | select the workspace manifest |
| inside workspace member + `MOON_NO_WORKSPACE=1` | ignore the workspace and use the nearest ancestor `moon.mod.json` |
| workspace root with no module + `MOON_NO_WORKSPACE=1` | error: workspace mode is disabled and no module is available |
| `moon work sync` outside a workspace | error: requires `moon.work` |
