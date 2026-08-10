# Behavior of `moon … --dry-run`

> **This output is intentionally unstable and meant only for maintainers.**
> Do not build tools or workflows that depend on the exact formatting or ordering.

- **Deterministic order**. Build commands are printed in a topologically sorted order; executing them sequentially produces the expected build artifacts.
- **Unix-style command lines**. Every command line is rewritten using Unix shell quoting, even on Windows hosts.
- **Backslash normalize**. Backslashes `\` in the commandline is normalized to forward slash `/`.
- **Home directory masking**. Any occurrence of the Moon home directory (`~/.moon` or a custom `$MOON_HOME`) is rewritten to the literal `$MOON_HOME`.
- **Project-relative paths**. Paths that live under the project root are emitted as a relative path from the project root, instead of absolute paths.
- **Best-effort path masking**. Project, Moon home, selected toolchain, and explicitly configured binary override paths are masked. Roots are replaced only when followed by a directory, end-of-value, or platform path-list boundary; missing or unresolvable override paths never make rendering fail.
- **Stable command programs**. A command program directly under the selected toolchain's `bin` directory is shortened to its file name. The running `moon` executable uses its file name. Other command programs remain unchanged unless an explicit override applies. Other occurrences of the same paths, such as build graph inputs, retain their masked or original path form. Platform executable suffixes on toolchain programs are omitted for stable cross-platform output.
- **Explicit binary overrides**. Only override environment variables that are set are resolved and masked with the corresponding variable name, such as `$MOONC_OVERRIDE`.
- **`moon run --dry-run` extras**. After the build commands, the dry-run output also prints the command that would execute the produced binary (typically `moonrun`, `node`, or the final executable).
- **`moon test --verbose` extras**. With `--verbose` set, `moon test` print the command that is executed for each test case.
