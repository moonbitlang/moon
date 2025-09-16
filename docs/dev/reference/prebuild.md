# Prebuild Tasks

Prebuild tasks let a package generate source files (typically `.mbt`) from other assets before compilation, via declarative rules in `moon.pkg.json`.

- Scope: Applies only to non–third‑party packages in the current module.
- Bypass: If `MOON_IGNORE_PREBUILD` is set in the environment, prebuild tasks are skipped.

## Package configuration

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
- `input`/`output` paths are package‑relative in config and expand to absolute paths inside the command.
- When arrays are used, placeholders expand to space‑separated lists in declaration order.
- Generated outputs are treated as regular package sources in the current build and categorized by filename rules.

## Placeholder substitution

Only the `command` field is substituted. The following placeholders are recognized:

- $input
  - Expands to a space‑separated list of absolute paths for all declared inputs (in order).
  - If `input` is a single string, this is one absolute path.

- $output
  - Expands to a space‑separated list of absolute paths for all declared outputs (in order).
  - If `output` is a single string, this is one absolute path.

- $mod_dir
  - Expands to the absolute path of the module root directory (the directory containing `moon.mod.json`).

- $pkg_dir
  - Expands to the absolute path of the current package directory (the directory containing this `moon.pkg.json`).

- $mooncake_bin
  - Expands to the absolute path `<module-root>/.mooncakes/__moonbin__`.

Substitution semantics:
- Pure textual replacement over the `command` string; unknown tokens are left as‑is.
- Paths expand to absolute, platform‑native strings; no quotes are added by substitution.
  - If paths may contain spaces or shell‑special characters, quote or escape them in the command template.
- For array `input`/`output`, items are joined by a single space in insertion order.

## :embed shorthand

If and only if the command starts with `:embed` (no leading whitespace), it denotes the built‑in embed tool. Conceptually this is equivalent to invoking the Moon tool's "embed" subcommand with the remaining arguments and substituted placeholders.

- Form: `:embed [FLAGS...] -i $input -o $output [--name IDENT]`
- Purpose: Generate output file(s) that embed the content of input file(s) as MoonBit code.
- Common flags:
  - `--text`: embed as escaped text literal(s).
  - `--binary`: embed as binary byte sequence(s).
  - `--name NAME`: set the generated identifier name in the produced code.
- Input/output arity:
  - Typical usage is one input → one output. If arrays are used, `$input`/`$output` expand to space‑separated lists; interpretation of multiple paths is defined by the embed tool’s CLI.

Behavioral notes:
- `:embed` is a syntactic alias; after substitution, it represents the same arguments a direct embed invocation would receive.
- The generated outputs are consumed as package sources in the same build.

## Path resolution

- `input`/`output` are resolved relative to the package directory before substitution.
- `$input`/`$output` expand to absolute file paths.
- `$pkg_dir`/`$mod_dir` expand to absolute directories.
- `$mooncake_bin` expands to `<module-root>/.mooncakes/__moonbin__`.

## Ordering and inclusion

- Tasks are processed in the order they appear in `pre-build`.
- All declared outputs are expected to be produced. Outputs are included as regular sources and categorized by filename.

## Failure conditions

For the purposes of the build, a task is considered failed if any of the following holds after substitution and tool semantics:

- Any declared `input` path does not exist.
- Any declared `output` cannot be created or written by the invoked tool.
- The invoked tool fails to produce all declared outputs.

## Compatibility and caveats

- `:embed` detection is prefix‑based and requires `:embed` as the first token.
- Substitution does not add quoting. Quote `$input`, `$output`, or directories inside `command` if paths may contain spaces.
- Using arrays for `output` expands to a space‑separated list; ensure your tool’s CLI accepts the intended arity.

## Examples

Text embedding:

```json
{
  "pre-build": [
    {
      "input": "assets/readme.txt",
      "output": "gen/readme_text.mbt",
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
      "output": "gen/logo_data.mbt",
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
      "output": "gen/all_texts.mbt",
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
      "output": "gen/something.mbt",
      "command": "custom-tool --assets \"$pkg_dir/assets\" --bin \"$mooncake_bin\" -i \"$input\" -o \"$output\""
    }
  ]
}