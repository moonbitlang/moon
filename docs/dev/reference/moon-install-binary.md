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
- hosted tree URL shortcuts (resolved to git URL + branch/path):
  - `https://github.com/<owner>/<repo>/tree/<ref>/<path...>`
  - `https://gitcode.com/<owner>/<repo>/tree/<ref>/<path...>`
  - `https://gitcode.com/<owner>/<repo>/-/tree/<ref>/<path...>`

Rules:

- Clone repo, optionally checkout ref.
- For hosted tree URL shortcuts:
  - repo URL is inferred from the owner/repo prefix,
  - `<ref>` is treated as branch when no explicit `--rev/--branch/--tag` is given,
  - `<path...>` is treated as `PATH_IN_REPO`.
- `PATH_IN_REPO` is interpreted as a filesystem path inside the cloned repository.
- If `SOURCE` already contains a tree path, `PATH_IN_REPO` positional argument must be omitted.
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

## Cross-tool structural reference (verified 2026-03-04)

This table compares how `moon install` (binary mode), `go install`, and `cargo install`
identify source, intent, and target.

| Dimension | `moon install` (binary mode) | `go install` | `cargo install` |
| --- | --- | --- | --- |
| Source identification | Resolution order: `--path` local override, then local-path-looking `SOURCE` (`./`, `../`, `/`, drive), then git URL, else registry package path. | Package args are import paths/patterns by default. Rooted paths or args beginning with `.`/`..` are interpreted as filesystem paths. With `@version`, args must be package paths/patterns (not relative/absolute paths). | Default source is crates.io. Source is switched explicitly via `--git`, `--path`, `--registry`, or `--index`. |
| Intent identification | Default intent: exact package install. Wildcard intent: `/...` suffix means install all matching `is-main: true` packages under prefix. | Default intent: exact package/import path(s). Wildcard intent: `...` package patterns expand to all matches; `x/...` includes `x` and descendants because wildcard can match empty string. | Exact crate install (no package-pattern wildcard like `...`). For a selected crate package, Cargo installs all binary targets by default (`--bins` behavior), or one via `--bin <name>`. |
| Target identification | Registry mode matches logical package path. Local/git mode matches filesystem path, then maps discovered package(s) under the module. | Target is package/import path (or package in a directory when filesystem path form is used). | Target is crate identity. If source contains multiple crates (registry/git), crate argument disambiguates. `--path` points to a local crate directory. |
| Relative path handling | Bare `foo/bar` is treated as registry package path. Use `./foo/bar` / `../foo/bar` for local filesystem semantics. | Rooted / `.` / `..`-prefixed args are filesystem paths; otherwise import-path semantics. | Relative filesystem path is only interpreted through `--path`; positional args are crate names. |
| Version / revision selection | Registry: `user/module/pkg@version`. Git: `--rev`, `--branch`, `--tag`. | `pkg@version` installs in module-aware mode and ignores current-module `go.mod` (subject to `go install` constraints). | Registry versions via `crate@version` or `--version`; git refs via `--branch`, `--tag`, `--rev`. |
| Build argument surface | Installer does not expose a broad package-manager-level build argument passthrough; it builds/installs selected native executables (with shared command flags like `--dry-run` and verbosity controls). | Accepts shared Go build flags: `go install [build flags] [packages]`. | Rich install/build controls: features (`--features`, `--all-features`, `--no-default-features`), target/profile (`--target`, `--profile`), jobs, lock/offline, etc. |
| Collision / overwrite behavior | Name collisions between selected packages are allowed; later installs overwrite (reserved Moon tool names are blocked). | Official help does not document a dedicated collision guard in install destination; binaries are installed by command name in one bin directory (inference). | Overwrite protection by default; `--force` is required to overwrite existing crates/binaries. |
| Dry-run / preview | `--dry-run` prints plan and does not write binaries. | `-n` (shared build flag) prints commands without executing. | `--dry-run` exists but is unstable. |
| Interactive prompting | No interactive package/binary selection prompt. | No interactive package selection prompt. | No interactive package/binary selection prompt. If source has multiple packages and target is ambiguous, Cargo errors and requires explicit package selection. |
| Install destination | `~/.moon/bin` by default, override with `--bin`. | `GOBIN`, else `$GOPATH/bin` (or `$HOME/go/bin` when `GOPATH` unset). | Install root `bin` directory; precedence: `--root`, `CARGO_INSTALL_ROOT`, config, `CARGO_HOME`, `$HOME/.cargo`. |

References:

- Go command docs (`install`, package lists/patterns): <https://pkg.go.dev/cmd/go>
- Go module reference (`go install pkg@version`): <https://go.dev/ref/mod#go-install>
- Cargo install reference: <https://doc.rust-lang.org/cargo/commands/cargo-install.html>

## Historical behavior before this clarification

The implementation previously had several surprising behaviors that caused confusion:

1. Selecting a module root in local/git mode could install all main packages implicitly.
2. Git path selection used string-based matching and could select the wrong package in nested-module repos.
3. `moon install --dry-run` still performed real build/install side effects.

This reference defines the expected behavior used to align code, tests, and user documentation.
