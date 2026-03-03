# Behavior of `moon install` (binary installer)

> This page documents the binary-install mode of `moon install`.
> It does not describe the legacy no-arg dependency-sync behavior.
>
> Status: expected behavior in this branch as of March 2, 2026.

## Scope

`moon install` has two modes:

1. `moon install` with no package arguments:
   legacy dependency install/sync for the current project (deprecated).
2. `moon install <package-selector> ...`:
   binary installer mode (this document).

## Selector model

Binary installer mode resolves a target source first, then selects package(s) in that source.

### Terminology: filesystem path vs package path

This document uses two different path notions:

1. **Filesystem path** (physical path):
   path on disk, e.g. `/repo/examples/pixeladventure` or `./cmd/tool`.
2. **Package path** (logical package identity inside a module):
   `module_name + "/" + package_relative_path`.

Important details for package paths:

- `package_relative_path` is relative to the module's `source` root, not necessarily module root.
- When `source` is set in `moon.mod.json`, the `source` directory name is not part of the package path.

Example:

- module root: `/repo`
- `moon.mod.json`: `{ "name": "user/proj", "source": "src" }`
- package directory on disk: `/repo/src/tools/fmt`
- package path: `user/proj/tools/fmt` (not `user/proj/src/tools/fmt`)

### Source resolution

Given `SOURCE`:

1. If `--path <PATH>` is set, use local path mode.
2. Else if `SOURCE` looks like a local path (`./`, `../`, `/`, or Windows drive), use local path mode.
3. Else if `SOURCE` looks like a git URL, use git mode.
4. Else, use registry mode.

Practical caveat:

- Bare relative strings like `foo/bar` are treated as registry package paths, not filesystem paths.
  Use `./foo/bar` (or `../foo/bar`) to force local path mode.

### Package selection

Default rule: install an exact package.

Wildcard rule: if selector ends with `/...` (or `...`), install all main packages under that prefix.

When matching packages after discovery:

- registry selectors are matched by **package path**,
- local/git selectors are matched by **filesystem path** (then converted to discovered packages).

## Expected behavior by mode

### Registry mode

Input form:

- `moon install user/module/pkg`
- `moon install user/module/...`
- optional `@version`, e.g. `user/module/pkg@1.2.3`

Rules:

- No wildcard: exact package path in the resolved registry module.
- With wildcard: all `is-main: true` packages under the wildcard prefix.

### Local path mode

Input form:

- `moon install --path ./some/pkg`
- `moon install ./some/pkg`
- wildcard via positional arg: `moon install ./some/prefix/...`

Rules:

- Input selector is a filesystem path.
- No wildcard: exact package at that filesystem path.
- If the exact filesystem path is module root, install that module's root package only
  (root package path is empty string relative to `source` root).
- With wildcard suffix: all `is-main: true` packages under the matched filesystem path prefix.

### Git mode

Input form:

- `moon install <git-url> [PATH_IN_REPO] [--rev|--branch|--tag]`
- wildcard in repo selector: `PATH_IN_REPO=some/prefix/...`

Rules:

- Clone repo, optionally checkout ref.
- `PATH_IN_REPO` is interpreted as a filesystem path inside the cloned repository.
- Resolve selected filesystem path and find nearest ancestor containing `moon.mod.json` as module root.
- No wildcard:
  - no `PATH_IN_REPO`: install root package of detected module.
  - with `PATH_IN_REPO`: install exact package at the selected filesystem path.
- With wildcard in `PATH_IN_REPO`: install all `is-main: true` packages under that filesystem prefix.
- Path escape (`..` resolving outside cloned repo) is rejected.

Practical caveat:

- If `PATH_IN_REPO` is omitted, the repository root must itself be a module root
  (contain `moon.mod.json`) for installation to proceed.
  For nested-module repositories, pass `PATH_IN_REPO` explicitly.

## Shared behavior

These rules apply in all binary installer modes:

- Only packages with `is-main: true` are installable.
- Build target is native executable in release mode.
- Output directory defaults to `~/.moon/bin` and can be overridden by `--bin`.
- Binary name:
  - last segment of package path for non-root packages
  - module unqualified name for root package
- Reserved Moon toolchain binary names are not overwritten.
- Name collisions between selected packages are allowed; later installs overwrite earlier files.
- `--dry-run` does not write binaries; it prints what would be built/installed.

## Historical behavior before this clarification

The implementation previously had several surprising behaviors that caused confusion:

1. Selecting a module root in local/git mode could install all main packages implicitly.
2. Git path selection used string-based matching and could select the wrong package in nested-module repos.
3. `moon install --dry-run` still performed real build/install side effects.

This reference defines the expected behavior used to align code, tests, and user documentation.
