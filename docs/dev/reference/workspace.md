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
  - an optional default backend for the workspace

Workspace members are canonicalized relative to the workspace root and deduped.

The workspace root may or may not also contain a `moon.mod.json`.

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
- `moon fmt`
- `moon info`

When the selected manifest is a workspace manifest, these commands operate on
the whole workspace. They can be run from:

- the workspace root
- a workspace member directory
- a nested non-module directory under the workspace
- `-C <member>`

They do not need an implicit default member.

Within this category, it helps to distinguish two subgroups:

- **Workspace-wide planning commands**:
  `moon build`, `moon check`
- **Workspace-wide inspection or transformation commands**:
  `moon test`, `moon fmt`, `moon info`

All of them operate on the selected project rather than a single member, but
`build` and `check` now have a more explicit workspace-wide planning model:

- they accept package/path selectors across the selected project,
- they may split one invocation into multiple backend-specific runs when
  `--target` is omitted,
- and they use `module preferred_target -> workspace preferred_target ->
  default backend` to decide those runs.

That makes them more than just "project-scoped"; they are the current
workspace-wide target-planning commands.

### Member-Scoped Commands

These commands need one concrete module even when the selected project is a
workspace.

Current examples:

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

At a workspace root, they fail unless Moon can determine a member module from
context. In practice, that means:

- running them directly at the workspace root is not supported
- running them from a member directory is supported
- passing `-C <member>` is supported

This is why `publish`, `package`, `doc`, and `prove` only work for one selected
module at a time today. There is no "publish the whole workspace" or "generate
docs for the whole workspace" mode in the current design.

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
- selecting a member module with `-C <member>`
- whole-workspace `build` / `check` / `test` / `fmt` / `info`
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
- `MOON_WORK`
- `MOON_NO_WORKSPACE` (deprecated fallback)

`--manifest-path` pins manifest resolution, but does not change the process
working directory.

## `MOON_WORK`

`MOON_WORK` is Moon's workspace-selection switch, intentionally close to
`GOWORK` in Go.

Accepted values:

- unset, empty, or `auto`
  - use normal ancestor-based workspace discovery
- `off`
  - disable workspace mode entirely
- a path to `moon.work`
  - pin selection to that workspace manifest

With `MOON_WORK=off`:

- implicit workspace discovery is disabled
- commands behave as if workspace mode does not exist

With `MOON_WORK=<path-to-moon.work>`:

- project-scoped commands use that workspace even outside the workspace tree
- if the current or explicitly selected `moon.mod.json` is not a workspace
  member, Moon fails instead of silently falling back

## `MOON_NO_WORKSPACE`

`MOON_NO_WORKSPACE` is deprecated.

- when `MOON_WORK` is unset, it is treated as a legacy alias for
  `MOON_WORK=off`
- when both are set, `MOON_WORK` wins

## Selection

Most commands start from the current directory after `-C`.

`moon run` is the exception: it resolves its positional selector path first,
then discovers the project from that selector location. That lets
`moon run path/to/pkg` and `moon run path/to/file.mbt` work even when invoked
outside the target project.

The algorithm is:

1. Canonicalize the current directory.
2. If `MOON_WORK=off` is enabled:
   - find the nearest ancestor containing `moon.mod.json`
   - if found, select that module manifest
   - otherwise, fail because workspace mode is disabled and no module exists
3. If `MOON_WORK` points to a `moon.work` file:
   - select that workspace manifest
   - if the nearest ancestor `moon.mod.json` exists and is not a workspace
     member, fail
4. Otherwise, walk ancestors from nearest to farthest and look for applicable
   workspace manifests.
5. If an applicable workspace manifest is found, select that workspace
   manifest.
6. If no applicable workspace is found, fall back to the nearest ancestor
   `moon.mod.json`.
7. If neither exists, fail with "not in a Moon project".

`--manifest-path` still exists as a deprecated compatibility flag for
non-`run` commands, but the public recommendation is to use `-C` instead.

## Selection With `--manifest-path`

`--manifest-path` accepts exactly:

- `moon.mod.json`
- `moon.work`

The selected manifest path is canonicalized first.

### `--manifest-path <...>/moon.mod.json`

This means "start from this module".

- if `MOON_WORK=off` is enabled, the command stays in single-module mode
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

If `MOON_WORK=off` is enabled:

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

With `MOON_WORK=off`, the workspace is ignored and selection falls back to
the nearest ancestor module, which is `outer/moon.mod.json`.

## Colocated `moon.work` And `moon.mod.json`

If a directory contains both:

- with workspace mode enabled, Moon may select the workspace manifest
- with `MOON_WORK=off`, Moon must select `moon.mod.json` instead

This was the bug shape that motivated the recent cleanup of workspace
selection.

## Examples

| Start point / flags | Result |
| --- | --- |
| workspace root + `build` / `check` / `test` / `fmt` / `info` | operate on the whole workspace |
| workspace root + `add` / `remove` / `tree` / `package` / `publish` / `doc` / `prove` | error: no target member can be inferred |
| member directory + member-scoped command | target that member and keep workspace context |
| `-C app` | start from `app`, then allow workspace promotion |
| `-C app` + member-scoped command | act on `app`, but keep workspace-local deps/layout if a workspace applies |
| `moon run path/to/app` from outside the project | discover the project from `path/to/app` |
| `--manifest-path app/moon.mod.json` | start from `app`, then allow workspace promotion |
| `--manifest-path app/moon.mod.json` + member-scoped command | act on `app`, but keep workspace-local deps/layout if a workspace applies |
| `--manifest-path moon.work` | select the workspace manifest |
| inside workspace member + `MOON_WORK=off` | ignore the workspace and use the nearest ancestor `moon.mod.json` |
| workspace root with no module + `MOON_WORK=off` | error: workspace mode is disabled and no module is available |
| anywhere + `MOON_WORK=/abs/path/to/moon.work` | pin selection to that workspace |
| `moon work sync` outside a workspace | error: requires `moon.work` |
