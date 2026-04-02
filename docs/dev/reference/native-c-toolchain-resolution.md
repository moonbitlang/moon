# Native C Toolchain Resolution

This document explains how Moon resolves and uses the C toolchain for native builds.

The resolved toolchain has three roles:

- compiler driver (`cc_path`)
- linker driver (also `cc_path`)
- archiver (`ar_path`)

Moon does not resolve a standalone linker executable (`ld`, `lld-link`) in this path.
Linking is performed through the selected compiler driver.

## Standard C Pipeline

The ordinary C pipeline is:

1. preprocess source files
2. compile/assemble them into object files
3. link object files and libraries into the final executable or shared library

When a user runs a command such as `clang foo.c -o foo`, these steps are usually fused by a single
compiler driver invocation. When a user runs `clang -c foo.c -o foo.o`, the pipeline stops at the
object file step and linking does not happen.

Moon uses both modes:

- it sometimes compiles a C file directly to an object file
- it sometimes invokes the compiler driver as a linker driver
- it sometimes invokes a separate archiver to group multiple object files into a static library

## What Moon Builds

For native-oriented backends, Moon does not only handle a single generated C file.
The final native artifact may involve multiple inputs:

- the output of `moonc link-core`
- the runtime implementation built from `runtime.c`
- package-level C stubs declared in `moon.pkg.json`

The high-level build flow is documented in `build.md`:

1. `BuildPackage`
2. `LinkCore`
3. `MakeExecutable`

For native backends, `LinkCore` emits:

- a generated C file for the Native backend
- an object file for the LLVM backend

Package C stubs are handled separately:

1. each `stub.c` is compiled to an object file
2. all stub object files in the package are archived together
3. the final executable links against that per-package archive

This is why Moon needs both a compiler driver and an archiver.

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

## Tool Roles and Compatibility

The main compatibility boundary is not "same operating system" or "same machine".
The relevant boundary is the target toolchain family and ABI.

In practice, the important dimensions are:

- target architecture
- object format
- ABI and runtime ecosystem
- command-line style of the selected tool driver

Examples:

- `cl` and `clang-cl` belong to the MSVC-style world
- `x86_64-w64-mingw32-gcc` belongs to the MinGW/GNU-style world
- `clang` can belong to either world depending on its target

This matters because two tools may both run on Windows while still expecting different flags,
different runtime libraries, or different default link behavior.

### Compiler driver vs linker driver

Moon treats the compiler as the entry point for linking:

- MSVC-style linking goes through `cl ... /link ...`
- GCC/Clang-style linking goes through `<cc> ... -o ...`

Moon does not currently resolve a standalone linker executable and then synthesize raw linker
command lines itself.

### Archiver

The archiver is separate from the linker.
Its role is to combine multiple object files into a static archive:

- GNU-style: `ar` or `llvm-ar`
- MSVC-style: `lib.exe`
- TCC-specific: `tcc -ar`

The archive is then consumed later by the linker driver.

## Flags Are Semantic, Not Just Syntactic

Tool selection is only half of the problem.
Even when two compilers share a similar name, the correct flags depend on the active toolchain
family and target environment.

Examples:

- `clang` is generally GCC-like in command-line syntax, but `clang` targeting `*-msvc` should not
  automatically receive Unix/GNU link assumptions such as `-lm`
- shared-library flags differ by family (`-shared` vs `/LD`)
- output flags differ by family (`-o` vs `/Fe...` or `/Fo...`)
- library search and runtime flags differ by family (`-L...`, `-Wl,...`, `/LIBPATH:...`)

For this reason, Moon must distinguish at least:

- how the tool is invoked
- what target environment the tool is producing code for
- whether the current step is compile, archive, or final link

The `-lm` fix belongs to this layer.
It is not primarily a "find the right binary" problem.
It is a "derive the right link semantics from the target toolchain" problem.

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

## Recommended Structure for the Implementation

The current implementation already has the right broad responsibilities, but maintainability improves
if future changes are evaluated in the following layers:

1. Tool discovery
   Decide which compiler driver and archiver binary to use.
2. Normalized toolchain description
   Record tool family, target triple, target environment, and archiver kind in a structured form.
3. Semantic command construction
   Build compile/link/archive requests from structured options such as output type, optimization
   level, debug info, runtime linkage mode, and required system libraries.
4. Textual flag emission
   Convert the semantic request into MSVC-style or GCC-style command lines.
5. User escape hatches
   Apply raw user-provided flags last, with the understanding that they are not guaranteed to be
   portable across toolchain families.

This split helps keep two separate classes of bugs from being conflated:

- "we picked the wrong tool"
- "we picked the right tool but emitted the wrong flags"
