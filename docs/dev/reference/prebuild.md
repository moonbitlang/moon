# Prebuild Tasks

Prebuild tasks let a package generate source files (typically `.mbt`) from other assets before compilation, via declarative rules in `moon.pkg.json`.

- Scope: Applies only to packages in the input module being built.
  Third-party dependencies are expected to already contain their generated outputs.

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

- `input`/`output` paths are package-relative in config and expand to absolute paths inside the command.
- When arrays are used, placeholders expand to space-separated lists in declaration order.
- All declared outputs are tracked as build outputs.
- Only `.mbt` and `.mbt.md` outputs are added back to the package's MoonBit source set.

## Path Resolution

- `input`/`output` are resolved relative to the package directory before substitution.
- `$input`/`$output` expand to absolute file paths.
- `$pkg_dir`/`$mod_dir` expand to absolute directories.
- `$mooncake_bin` expands to `<module-root>/.mooncakes/__moonbin__`.
  This directory contains launchers installed for the current module's direct
  `bin-deps`.

## Placeholder Substitution

Only the `command` field is substituted. The following placeholders are recognized:

- `$input`  
  Expands to a space-separated list of absolute paths for all declared inputs (in order). If `input` is a single string, this is one absolute path.

- `$output`  
  Expands to a space-separated list of absolute paths for all declared outputs (in order). If `output` is a single string, this is one absolute path.

- `$mod_dir`  
  Expands to the absolute path of the module root directory (the directory containing `moon.mod.json`).

- `$pkg_dir`  
  Expands to the absolute path of the current package directory (the directory containing this `moon.pkg.json`).

- `$mooncake_bin`  
  Expands to the absolute path `<module-root>/.mooncakes/__moonbin__`.
  This currently refers to launchers installed from the current module's direct
  `bin-deps`.

Substitution semantics:

- Pure textual replacement over the `command` string; unknown tokens are left as-is.
- Paths expand to absolute, platform-native strings; no quotes are added by substitution.
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
- The generated outputs are consumed as package sources in the same build.

## Ordering and Inclusion

- Each `pre-build` entry becomes its own prebuild node in the build graph.
- There is no explicit build-graph edge between two prebuild tasks solely because one is
  written earlier in `pre-build`.
- If one prebuild task consumes another task's declared output, the dependency is currently
  expected to be handled by the underlying file-level tracking in `n2`.
- Only declared outputs ending in `.mbt` or `.mbt.md` are included as MoonBit sources for
  compilation, and only when they live in the package directory itself.

## Failure Conditions

A task is considered failed (for the build) if any of the following holds after substitution and tool semantics:

- Any declared `input` path does not exist.
- Any declared `output` cannot be created or written by the invoked tool.
- The invoked tool fails to produce all declared outputs.

## Compatibility and Caveats

- `:embed` detection is prefix-based and requires the literal prefix `:embed `.
- Substitution does not add quoting. Quote `$input`, `$output`, or directories inside `command` if paths may contain spaces.
- Using arrays for `output` expands to a space-separated list; ensure your tool’s CLI accepts the intended arity.
- On Unix-like platforms, the final prebuild command string is executed through `sh -c`.
- On Windows, the final prebuild command string is passed directly to `CreateProcessA`.
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
