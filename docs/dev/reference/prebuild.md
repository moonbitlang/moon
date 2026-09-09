# Prebuild Tasks

Prebuild tasks let a package generate source files (typically `.mbt`) from other assets before compilation, via declarative rules in `moon.pkg.json`.

- Scope: Applies only to packages in the input module being built.
  Third-party dependencies are expected to already contain their generated outputs.
- The deprecated bin-dep compatibility build does not run package-level
  prebuild, including custom `pre-build`, `moonlex`, and `moonyacc`, even
  though its child build presents the distributed module as an input module.
  Published packages must contain their generated outputs.
- Package-level prebuild tasks are separate from the experimental module-level
  prebuild configuration script. The latter may still run for a native or LLVM
  bin-dep build to produce build configuration such as native link flags.

## Module-Level Prebuild Configuration

The `--moonbit-unstable-prebuild` field names a script relative to the module
root. Supported extensions select the runner:

| Extension | Runner |
| --- | --- |
| `.js`, `.cjs`, `.mjs` | Node.js |
| `.py` | Python |
| `.mbtx` | Compiled to Wasm, then executed with Moonrun |

For example:

```json
{
  "name": "username/project",
  "--moonbit-unstable-prebuild": "build.mbtx"
}
```

Every runner uses the module root as its working directory and receives the
captured process environment with these additional variables:

| Variable | Value |
| --- | --- |
| `MOON_MOD` | Absolute path to the module's selected `moon.mod` or `moon.mod.json` manifest |
| `MOON_BUILD_DIR` | Absolute path to this module's writable prebuild output directory |
| `MOON_HOST_OS` | OS of the Moon process: `linux`, `macos`, or `windows` |
| `MOON_HOST_ARCH` | Architecture of the Moon process, using names such as `x86_64` and `aarch64` |
| `MOON_BACKEND` | Resolved project backend: `native` or `llvm` |
| `MOON_PROFILE` | Effective project profile: `debug` or `release` |
| `MOON_JOBS` | Build job limit as a decimal integer, honoring `-j` / `--jobs` |

These variables override any inherited values of the same names. Scripts can
read their inputs entirely from the environment. For compatibility, stdin still
supplies the original JSON shape: `env` contains the captured environment before
these overrides, and `paths` contains `module_root` and `out_dir`. `module_root`
remains the module directory; `out_dir` now contains the real directory provided
as `MOON_BUILD_DIR`.

Host OS and architecture describe the Moon process's platform. Backend and
profile describe the project build, even when the prebuild script itself is
compiled to Wasm. They reflect effective
[command defaults](build.md#default-cli-profiles), manifest preferences, and
explicit CLI overrides.

When `--jobs` is omitted, prebuild scripts and the build executor share the same
available-parallelism observation, captured once per Moon process. If that
observation is unavailable, the limit defaults to one. `MOON_JOBS` is a build
concurrency limit, not a physical CPU count or a jobserver protocol.

`MOON_MOD` reports the manifest using the same preference as module discovery:
`moon.mod` takes precedence over `moon.mod.json`. It is supplied to prebuild
scripts as information, not read by Moon as a module-selection override.
[`MOON_WORK`](workspace.md#moon_work) remains the workspace-selection switch.

Moon creates `MOON_BUILD_DIR` before running the script. It resides under the
caller's target directory (including `--target-dir`), scoped by backend, profile,
command, and module. Files persist across repeated builds in that scope; scripts
must not assume the directory is empty. Dependencies receive their own output
directories under the consumer's target directory, so generated files need not
be written into dependency sources. The directory's internal layout is not part
of the script API.

Stdout must contain one build configuration JSON value with the optional fields
`vars`, `link_configs`, and `rerun_if`; `rerun_if` currently has no effect. Stderr
carries script diagnostics. A failed script or invalid JSON output fails the
build.

MoonBit scripts use standalone `.mbtx` imports and incremental compilation.
Their target is always linear-memory Wasm, independently of the project backend;
compiling the script therefore skips module-level prebuild configuration. The
command layer calls the standalone builder directly and preserves the outer
command's `--frozen` setting. Moonrun executes the compiled artifact, using normal
runtime discovery (including `MOONRUN_OVERRIDE`). Build progress stays off stdout
to preserve the JSON protocol.

Script compilation uses a stable per-script directory under the caller's target
directory, including when `--target-dir` is supplied. Both compiler artifacts and
private `.mooncakes` dependencies live there, so compilation does not write into
the script's source directory. This also applies to scripts in registry
dependencies shared between consumers.

The ordinary [embedded `.mbtx` policy](moonx.md#standalone-mbtx) applies. Policy
filesystem roots are relative to the script directory, while the working
directory remains the module root. An environment policy filters the script's
environment, including the variables above, but does not filter the values
explicitly supplied in the compatibility JSON input.

`--moonbit-unstable-prebuild` in `moon.mod.json` supplies dynamic native build
configuration, such as toolchain selection and compiler or linker flags. It runs
only for the Native and LLVM target backends. Wasm, WasmGC, and JS builds skip
it, including `moon build --target wasm --release` and dry runs. This applies
to both project and standalone-file builds, using the resolved target backend.
Each backend planning pass runs the scripts separately, with `MOON_BACKEND`
identifying that pass's backend.

`moon check` skips this configuration for the checked project on every backend.
Dependency installation can still invoke a separate native or LLVM build whose
configuration script runs. Native and LLVM dry runs also run these scripts,
because their output is needed to construct build commands.

Unlike package-level `pre-build` / `dev_build` code generation, whose outputs
should be generated before distribution, this configuration depends on the
consumer's build environment. `MOON_IGNORE_PREBUILD` continues to control
package-level generation; it does not suppress native or LLVM module-level
configuration scripts.

## Package Configuration

Define tasks in the `pre-build` array of `moon.pkg.json`:

```json
{
  "pre-build": [
    {
      "input": "path/to/input.ext",
      "output": "path/to/output.mbt",
      "command": "..."
    }
  ]
}
```

- input: string or array of strings; paths are relative to the package directory.
- output: string or array of strings; paths are relative to the package directory.
- command: a string that supports placeholder substitution (below).

Notes:

- `input`/`output` paths are package-relative in config and expand to prebuild-cwd-relative paths inside the command.
- When arrays are used, placeholders expand to space-separated lists in declaration order.
- During ordinary input-module builds that plan package prebuild, each planned
  action's declared outputs are tracked as build outputs. Bin-dep compatibility
  builds do not create package-level prebuild actions; they consume existing
  `.mbt` and `.mbt.md` outputs as package sources.
- Outputs of actions in the current `PackagePrebuildPlan` go through the same
  File Interpretation as paths in the Package File Set. `.mbt` becomes a
  conditionally classified MoonBit source, `.mbt.md` becomes a blackbox input,
  `.mbtp` becomes a check/prove proof input, and `.mbl`/`.mby` schedules
  moonlex/moonyacc whose generated `.mbt` is then classified as a source.
  A declaration alone does not make an absent output a plan input when package
  prebuild policy does not apply, as in bin-dep compatibility builds.
- `MOON_IGNORE_PREBUILD` disables applicable custom actions and does not add
  their declared outputs to File Interpretation. Generated files already
  present in the Package File Set remain ordinary compiler inputs.
- A virtual package's expected `pkg.mbti` or legacy contract path may be a
  planned prebuild output. Build planning selects the preferred available
  contract after package prebuild actions are known. Other output extensions
  remain ordinary tracked files and can be consumed by later prebuild actions
  through matching paths.

## Path Resolution

- `input`/`output` are resolved relative to the package directory for build dependency tracking.
- `$input`/`$output` expand to paths relative to the prebuild command working directory. These substituted paths are lexically normalized and start with `./`, for example `./src/lib/input.txt`.
- `$pkg_dir`/`$mod_dir` expand to absolute directories.
- `$mooncake_bin` expands to `<project target dir>/__moonbin__`.
  Project discovery computes this `mooncake_bin_dir` with the other
  authoritative project directories. Command adapters pass it unchanged
  through dependency sync and build planning; downstream stages do not
  reconstruct it from another directory. `bin-deps` is deprecated; new tools
  should be published as portable Wasm executable packages and run with
  `moonx` instead.

## Placeholder Substitution

Only the `command` field is substituted. The following placeholders are recognized:

- `$input`  
  Expands to a space-separated list of normalized prebuild-cwd-relative paths for all declared inputs (in order). If `input` is a single string, this is one path.

- `$output`  
  Expands to a space-separated list of normalized prebuild-cwd-relative paths for all declared outputs (in order). If `output` is a single string, this is one path.

- `$mod_dir`  
  Expands to the absolute path of the module root directory (the directory containing `moon.mod.json`).

- `$pkg_dir`  
  Expands to the absolute path of the current package directory (the directory containing this `moon.pkg.json`).

- `$mooncake_bin`  
  Expands to the absolute path `<project target dir>/__moonbin__`.
  This refers to launchers installed from the current project's direct
  `bin-deps`.

Substitution semantics:

- Pure textual replacement over the `command` string; unknown tokens are left as-is.
- `$input` and `$output` expand to `./...` paths relative to the prebuild command working directory; no quotes are added by substitution.
- Directory placeholders expand to absolute, platform-native strings.
  - If paths may contain spaces or shell-special characters, quote or escape them in the command template.
- For array `input`/`output`, items are joined by a single space in insertion order.

## :embed Shorthand

If the command starts with the exact prefix `:embed `, it denotes the built-in embed tool.
Conceptually this is equivalent to invoking the Moon tool `embed` subcommand with
the remaining arguments and substituted placeholders.

- Form: `:embed [FLAGS...] -i $input -o $output [--name IDENT]`
- Purpose: Generate output file(s) that embed the content of input file(s) as MoonBit code.
- Common flags:
  - `--text`: embed as escaped text literal(s).
  - `--binary`: embed as binary byte sequence(s).
  - `--name NAME`: set the generated identifier name in the produced code.
- Input/output arity:
  - Typical usage is one input → one output. If arrays are used, `$input`/`$output` expand to space-separated lists; interpretation of multiple paths is defined by the embed tool’s CLI.

Behavior:

- `:embed` is a syntactic alias; after substitution, it represents the same arguments a direct embed invocation would receive.
- Moon lowers `:embed` directly to structured `moon tool embed` arguments; it does not execute the built-in invocation through a shell.
- The argument tail supports platform-native quoting and escaping, but not shell expansion, redirection, pipelines, or command lists. Use a custom prebuild command when platform-specific shell behavior is required.
- The generated outputs are consumed as package sources in the same build.

## Ordering and Inclusion

- During ordinary input-module builds, each backend-specific Build Plan owns a
  separate `PackagePrebuildPlan` containing the requested custom, moonlex, and
  moonyacc actions. Bin-dep compatibility builds create none.
- `PackagePrebuildPlan` owns package file-generation actions as a peer of the
  `BackendPlan`; it is not a separate semantic artifact model. Package
  prebuild is therefore never represented as a backend node with an
  inapplicable target kind.
- There is no explicit edge between two prebuild tasks solely because one is
  written earlier in `pre-build`. Their concrete inputs and outputs form a
  dependency only when the paths match in the completed Execution Plan.
- Build Target Projection includes generated `.mbt`, `.mbt.md`, `.mbtp`, and
  selected virtual `.mbti` paths in the same package file sets as discovered
  paths. These package files are not Build Artifacts.
- All planned package prebuild actions remain part of normal execution.
  Declared outputs that no backend action consumes are leaf outputs of the n2
  graph, so they are still requested and the invocation waits for them.
- Generated `.mbt`/`.mbt.md`, `.mbtp`, `.mbl`/`.mby`, and the selected virtual
  `.mbti` path retain the meanings described above. Lowering records them as
  concrete input and output observations; matching paths recover the producer
  relationship without a parallel package-file identity.
- Each backend-specific Execution Plan still contains one complete package
  prebuild action. When the command layer composes a multi-backend invocation,
  it shares those providers by identical physical outputs and complete
  execution behavior before constructing the invocation's single n2 graph.

## Environment Capture

- Module-level prebuild configuration scripts receive a snapshot of the process
  environment captured by `rr_build` before prebuild execution.
- Commands that skip prebuild configuration do not capture this environment.
- The snapshot is passed to each module prebuild script through its environment
  and the compatibility stdin input; it is not rediscovered inside individual
  module execution. The module paths are added as environment variables after
  applying the snapshot.

## Failure Conditions

When a task is scheduled, it is considered failed if any of the following
holds after substitution and tool semantics:

- Any declared `input` path does not exist.
- Any declared `output` cannot be created or written by the invoked tool.
- A built-in `:embed` argument tail contains malformed native quoting or escaping.
- The invoked tool fails to produce all declared outputs.

## Compatibility and Caveats

- `:embed` detection is prefix-based and requires the literal prefix `:embed `.
- Substitution does not add quoting. Quote `$input`, `$output`, or directories inside `command` if paths may contain spaces.
- Using arrays for `output` expands to a space-separated list; ensure your tool’s CLI accepts the intended arity.
- On Unix-like platforms, custom prebuild command strings are executed through `sh -c`.
- On Windows, custom prebuild command strings are passed directly to `CreateProcessW`.
- The `:embed` shorthand is a standalone structured built-in invocation on every platform and does not use either custom-command path. Shell syntax in its argument tail is never evaluated.
- On Unix-like platforms, if the first word of the command looks like a relative path,
  it is currently resolved against the module root directory.
- Windows (PowerShell):  
  On Windows only, if the first word of the `command` (before any spaces) corresponds to a `.ps1` file that exists in the module root directory, the command is rewritten to execute that script via PowerShell using its absolute path. Example: if the command is `generate-assets $input $output` and `generate-assets.ps1` exists in the module root, the effective command becomes `powershell <absolute-path-to-module-root>/generate-assets.ps1 $input $output`. Detection examines the first word before placeholder substitution.

## Examples

Text embedding:

```json
{
  "pre-build": [
    {
      "input": "assets/readme.txt",
      "output": "readme_text.mbt",
      "command": ":embed --text -i $input -o $output --name readme_text"
    }
  ]
}
```

Binary embedding:

```json
{
  "pre-build": [
    {
      "input": "assets/logo.bin",
      "output": "logo_data.mbt",
      "command": ":embed --binary -i $input -o $output --name logo_data"
    }
  ]
}
```

Multiple inputs (tool interprets list):

```json
{
  "pre-build": [
    {
      "input": ["assets/a.txt", "assets/b.txt"],
      "output": "all_texts.mbt",
      "command": ":embed --text -i $input -o $output --name all_texts"
    }
  ]
}
```

Location placeholders:

```json
{
  "pre-build": [
    {
      "input": "assets/something.dat",
      "output": "something.mbt",
      "command": "custom-tool --assets \"$pkg_dir/assets\" --bin \"$mooncake_bin\" -i \"$input\" -o \"$output\""
    }
  ]
}
```
