# Behavior of `moon … --dry-run`

> **This output is intentionally unstable and meant only for maintainers.**
> Do not build tools or workflows that depend on the exact formatting or ordering.

- **Deterministic order**. Build commands are printed in a topologically sorted order; executing them sequentially produces the expected build artifacts.
- **Unix-style command lines**. Every command line is rewritten using Unix shell quoting, even on Windows hosts.
- **Backslash normalize**. Backslashes `\` in the commandline is normalized to forward slash `/`.
- **Home directory masking**. Any occurrence of the Moon home directory (`~/.moon` or a custom `$MOON_HOME`) is rewritten to the literal `$MOON_HOME`.
- **Project-relative paths**. Paths that live under the project root are emitted as a relative path from the project root, instead of absolute paths.
- **Toolchain binary aliases**. Known Moon toolchain executables (e.g. `~/.moon/bin/moonc`) are shortened to their bare names (`moonc`). Other executables keep their original paths.
- **`moon run --dry-run` extras**. After the build commands, the dry-run output also prints the command that would execute the produced binary (typically `moonrun`, `node`, or the final executable).
- **`moon test --verbose` extras**. With `--verbose` set, `moon test` print the command that is executed for each test case.
