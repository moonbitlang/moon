# Native C Toolchain Resolution

This document explains how Moon resolves and uses the C toolchain for native builds.

The resolved toolchain has three roles:

- compiler driver (`cc_path`)
- linker driver (also `cc_path`)
- archiver (`ar_path`)

Moon does not resolve a standalone linker executable (`ld`, `lld-link`) in this path.
Linking is performed through the selected compiler driver.

## Resolution Order

Tool resolution starts from `crates/moonutil/src/compiler_flags.rs`.

### 1. Environment override

- If `MOON_CC` is set, Moon uses it as the compiler.
- If `MOON_AR` is set together with `MOON_CC`, Moon uses it as the archiver directly.
- `MOON_CC` override takes precedence over user-provided compiler options in command settings.

### 2. Auto-detection (when `MOON_CC` is unset)

Moon probes in this order:

1. `cl`
2. `cc`
3. `gcc`
4. `clang`
5. internal `tcc`

### 3. Compiler kind detection

Compiler kind matching is case-insensitive (`cl`, `gcc`, `clang`, `tcc`, `cc`),
but filename/path text is preserved for path synthesis fallback.

## Archiver Resolution

### MSVC (`cl`)

- Archiver is `lib.exe`.

### TCC

- Uses `tcc -ar` mode (no separate archiver binary).

### GCC/System CC

- Uses suffix-based fallback (`...gcc...`/`...cc...` -> `...ar...`), preserving original filename casing.

### Clang

For correctness, Moon first asks Clang for tool paths and validates existence:

1. `clang -print-prog-name=ar`
2. if unresolved or non-existent, `clang -print-prog-name=llvm-ar`
3. if still unresolved, fallback to suffix conversion (`...clang...` -> `...ar...`, case-preserving)

Validation rule:

- If Clang reports an absolute/qualified path, it must exist as a file.
- If Clang reports a bare executable name, it must be resolvable via `PATH`.

This avoids trusting non-existent `ar` reports on platforms where only `llvm-ar` is installed.

## Compile, Link, and Archive Usage

### Compile

`make_cc_command*` uses `cc_path` for compile steps (`-c` or `/c`) and backend-specific flags.

### Link

`make_linker_command*` also uses `cc_path` to drive linking:

- MSVC style: `cl ... /link ...`
- GCC/Clang style: `<cc> ... -o ...`

`-lm` is only added for gcc-like compilers when target triple does not contain `msvc`.

### Archive

`make_archiver_command*` uses resolved `ar_path`:

- `lib.exe` (MSVC)
- `ar`/`llvm-ar` (gcc/clang-like)
- `tcc -ar` (TCC)

## Design Intent

Resolution strategy is:

1. explicit user override
2. compiler-reported tool discovery
3. compatibility fallback

This ordering prioritizes correctness while preserving compatibility with existing toolchain layouts.
