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

The selected project is represented explicitly. A module selection carries its
module manifest path; a workspace selection carries one completed workspace
layout.

## Manifest Model

Moon accepts one workspace manifest format:

- `moon.work`
  - the current DSL form

A module manifest may use either supported form:

- `moon.mod`
  - the current DSL form
- `moon.mod.json`
  - the legacy JSON form

A workspace manifest defines:

- `members`
  - the module directories contained by the workspace

Workspace members are canonicalized relative to the workspace root and deduped.
Project discovery reads the selected `moon.work` and resolves its members once;
dependency sync, package discovery, and workspace maintenance receive that
completed layout instead of reopening the manifest. `preferred_target` in
`moon.work` is deprecated: commands warn once when they select that layout, but
they do not use it for backend selection. `moon fmt` removes it. Use
module-level `preferred_target` instead.

The workspace root may or may not also contain a module manifest.

## Selection Result

`ProjectQuery` resolves command input into a `SelectedProject` containing:

- a `ProjectContext`, with:
  - `project_manifest_path`: one of `moon.mod`, `moon.mod.json`, or `moon.work`
  - `project_root`: the parent directory of `project_manifest_path`
  - `module_dir`: `Some(member_dir)` when the selected workspace explicitly
    lists the module containing the command's start point, including a
    colocated root module; otherwise `None`
- a completed workspace layout when `project_manifest_path` is `moon.work`,
  containing the parsed manifest and canonical member directories

When commands create `PackageDirs`, this becomes a `ProjectManifest` value:

- `ProjectManifest::Module(path)`
  - single-module behavior
- `ProjectManifest::Workspace(layout)`
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
- and they use `module preferred_target -> default backend` to decide those
  runs.

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

Machine-readable `moon tree --json` output derives each `workspace_member`
value from the selected workspace layout, including workspaces with one member.
The human tree preserves its historical display convention and adds
`[workspace member]` only when the resolution has multiple workspace roots;
that text marker is not the authoritative membership signal.

But they are not workspace-wide commands.

At a workspace root, they fail unless Moon can determine a member module from
context. In practice, that means:

- running them directly at the workspace root is supported when that root also
  contains a module manifest and `moon.work` explicitly lists `.` as a member
- running them at any other workspace root is not supported
- running them from a member directory is supported
- passing `-C <member>` is supported

This is why `publish`, `package`, `doc`, and `prove` only work for one selected
module at a time today. There is no "publish the whole workspace" or "generate
docs for the whole workspace" mode in the current design.

`moon doc` also selects its Target Backend from that member module's
`preferred_target`, falling back to the default backend when it is absent.
Preferences from unrelated workspace members do not affect documentation
planning.

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
- workspace roots that also contain a module manifest
- selecting a colocated root module when `moon.work` explicitly lists `.`
- selecting a member module from inside the member directory
- selecting a member module with `-C <member>`
- whole-workspace `build` / `check` / `test` / `fmt` / `info`
- member-scoped `package` / `publish` / `doc` / `prove` while still using
  workspace-local dependency resolution
- nested discovery from non-module directories inside a workspace

The current design does not support:

- an implicit default member at workspace root for member-scoped commands;
  selecting a colocated root module requires an explicit `.` member
- workspace-wide `publish`
- workspace-wide `package`
- workspace-wide `doc`
- workspace-wide `prove`
- workspace-wide `add` / `remove` / `tree`

Those commands need a selected member. At a workspace root without a colocated
module explicitly listed as `.`, they fail with the "cannot infer a target
module in workspace" error.

## Selection Inputs

Project selection depends on:

- the working directory after `-C`
- `MOON_WORK`
- `MOON_NO_WORKSPACE` (deprecated fallback)

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
- if the module selected from the current directory is not a workspace member,
  Moon fails instead of silently falling back

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
   - find the nearest ancestor containing `moon.mod` or `moon.mod.json`
   - if found, select that module manifest
   - otherwise, fail because workspace mode is disabled and no module exists
3. If `MOON_WORK` points to a `moon.work` file:
   - select that workspace manifest
   - if the nearest ancestor module exists and is not a workspace member, fail
4. Otherwise, walk ancestors from nearest to farthest and look for applicable
   workspace manifests.
5. If an applicable workspace manifest is found, select that workspace
   manifest.
6. If no applicable workspace is found, fall back to the nearest ancestor
   module manifest.
7. If neither exists, fail with "not in a Moon project".

## How Moon Decides Whether A Workspace Applies

Workspace applicability is order-sensitive.

Moon walks ancestors from nearest to farthest. While walking, it derives the
current module boundary from the same ancestor order instead of precomputing one
outer module root before the walk.

This preserves the intended precedence:

- a nearer applicable workspace should win
- a farther workspace may still apply later if it explicitly lists the selected
  module as a member
- an unrelated outer module manifest must not make Moon skip a nearer workspace
  that should still apply

This differs from Go workspace discovery. Go selects the nearest ancestor
`go.work` whenever workspace mode is enabled, even when the module containing
the working directory is not listed. Moon instead lets a module boundary reject
unrelated ancestor workspaces.

After selecting a workspace, Moon exposes a selected module to member-scoped
commands only when that workspace explicitly lists the module.

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

## Colocated `moon.work` And A Module Manifest

If a directory contains both:

- with workspace mode enabled, Moon selects the workspace manifest
- if the workspace lists the colocated module, that module is also selected for
  member-scoped commands
- otherwise, Moon warns that the module is not a workspace member; workspace-wide
  commands still use the workspace, while member-scoped commands cannot infer a
  target module from that directory; the warning is a User Log and is suppressed
  by `--quiet`
- with `MOON_WORK=off`, Moon must select the colocated module manifest instead

This was the bug shape that motivated the recent cleanup of workspace
selection.

## Examples

| Start point / flags | Result |
| --- | --- |
| workspace root + `build` / `check` / `test` / `fmt` / `info` | operate on the whole workspace |
| workspace root with a module manifest whose `moon.work` lists `.` + member-scoped command | target the colocated module and keep workspace context |
| any other workspace root + member-scoped command | error: no target member can be inferred |
| member directory + member-scoped command | target that member and keep workspace context |
| `-C app` | start from `app`, then allow workspace promotion |
| `-C app` + member-scoped command | act on `app`, but keep workspace-local deps/layout if a workspace applies |
| `moon run path/to/app` from outside the project | discover the project from `path/to/app` |
| inside workspace member + `MOON_WORK=off` | ignore the workspace and use the nearest ancestor module manifest |
| workspace root with no module + `MOON_WORK=off` | error: workspace mode is disabled and no module is available |
| anywhere + `MOON_WORK=/abs/path/to/moon.work` | pin selection to that workspace |
| `moon work sync` outside a workspace | error: requires `moon.work` |
