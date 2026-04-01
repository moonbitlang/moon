# Native C Toolchain Resolution

This document explains how Moon currently resolves and uses the native C toolchain.

The resolved toolchain has three roles:

- compiler driver (`cc_path`)
- linker driver (also `cc_path`)
- archiver (`ar_path`)

Moon does not resolve a standalone linker executable such as `ld` or `lld-link` in this path.
Linking is performed through the selected compiler driver.

## Standard C Pipeline

The ordinary C pipeline is:

1. preprocess source files
2. compile or assemble them into object files
3. link object files and libraries into the final executable or shared library

When a user runs `clang foo.c -o foo`, these steps are usually fused by one compiler-driver
invocation. When a user runs `clang -c foo.c -o foo.o`, the pipeline stops after object generation.

Moon uses both modes:

- it compiles some C files directly to object files
- it invokes the compiler driver again for final linking
- it sometimes invokes a separate archiver to group multiple object files into a static archive

## What Moon Builds

For native-oriented backends, the final artifact may involve multiple inputs:

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
2. usually, all stub object files in the package are archived together
3. the final executable links against that per-package archive

`NativeTccRun` is the exception: instead of creating a static archive, Moon links the stub objects
into a shared library so `tcc -run` can load them at runtime.

This is why Moon needs both a compiler driver and an archiver.

## Resolution Layers

Tool resolution starts from `crates/moonutil/src/compiler_flags.rs`.

There are three sources of C toolchain selection:

1. global environment override
2. package-level override
3. default auto-detection

When a native build step chooses its compiler, the current precedence is:

1. `MOON_CC` / `MOON_AR`
2. package-level override (`link.native.cc` or `link.native.stub_cc`)
3. detected default toolchain

## Global Environment Override

- If `MOON_CC` is set, Moon uses it as the compiler for the regular native pipeline.
- `NativeTccRun` is the exception: its run-driver build step and runtime launcher always use the
  internal `tcc`, so `MOON_CC` does not affect those steps.
- If `MOON_AR` is set together with `MOON_CC`, Moon passes it into compiler resolution for the
  regular native pipeline.
- For `cc`, `gcc`, and `clang`, that means the resolved archiver path comes from `MOON_AR`.
- For `cl`, Moon still resolves `lib.exe` next to the compiler, so `MOON_AR` is ignored.
- For `tcc`, Moon still uses `tcc -ar`, so `MOON_AR` is ignored.
- `MOON_CC` takes precedence over package-level compiler overrides.
- Explicit `MOON_CC` values do not receive any synthesized MSVC environment overlay. Moon assumes
  the caller is intentionally choosing and configuring that toolchain.

This override is global to the current Moon invocation, except for the `NativeTccRun` tool choice.

## Package-Level Override

`moon.pkg.json` may specify native compiler overrides:

- `link.native.cc`
- `link.native.stub_cc`

These are parsed into `CC` values during build-plan construction.

They are useful as escape hatches, but they are toolchain-specific, not portable configuration.
For example, `cc = "cl"` is MSVC-specific, while `cc = "gcc"` or GNU-style flags are specific to
other toolchain families.

Package-level compiler overrides also do not receive any synthesized MSVC environment overlay. They
run in the current process environment, just like other explicit tool choices.

## Default Auto-Detection

When `MOON_CC` is unset and no package-specific override is being applied to that step, Moon probes
for a default native toolchain in this order:

1. `cl`
2. `cc`
3. `gcc`
4. `clang`
5. on Windows only, `find-msvc-tools` lookup for `cl.exe`
6. internal `tcc`

The Windows-only `find-msvc-tools` step exists for shells where Visual Studio tools are installed
but `cl.exe` is not already available on `PATH`.

If that fallback succeeds, Moon records the discovered MSVC environment and scopes it to wrapped
native C commands only. In practice, the build graph writes a serialized environment file under
`_build/.moon/` and executes those native commands through `moon tool env-exec`.

This scoped environment is only attached to the auto-detected default MSVC toolchain. It is not
applied to explicit `MOON_CC` selections or package-level compiler overrides.

## Compiler Kind Detection

Compiler kind matching is based on the executable filename and is case-insensitive.

Current recognized suffix families are:

- `...cl`
- `...gcc`
- `...clang`
- `...tcc`
- `...cc`

This is suffix-based, so prefixed tool names such as `x86_64-w64-mingw32-clang.exe` are still
recognized as Clang.

Filename text is preserved when Moon later needs a case-preserving fallback path.

## Archiver Resolution

### MSVC (`cl`)

- Archiver is `lib.exe`.

### TCC

- Uses `tcc -ar` mode.
- There is no separate archiver binary in this path.

### GCC and System CC

- Uses suffix-based fallback (`...gcc...` or `...cc...` -> `...ar...`).
- The fallback preserves the original filename casing.

### Clang

Clang archiver resolution has an extra discovery step:

1. `clang -print-prog-name=ar`
2. if that does not resolve to an existing tool, `clang -print-prog-name=llvm-ar`
3. if that also fails, fallback to suffix conversion (`...clang...` -> `...ar...`)

The fallback still preserves the original filename casing.

Moon validates compiler-reported tools before using them:

- absolute or path-like outputs must resolve to an existing file
- bare names must be resolvable through `PATH`
- on Windows, a reported path without `.exe` is also accepted if the corresponding `.exe` exists

This avoids trusting nonexistent `ar` reports on installations where only `llvm-ar` is available.

## Toolchain Families and Compatibility

The main compatibility boundary is not simply "same operating system" or "same machine".
The relevant boundary is the target toolchain family and ABI.

In practice, the important dimensions are:

- target architecture
- object format
- ABI and runtime ecosystem
- command-line style of the selected tool driver

Examples:

- `cl` and `clang-cl` belong to the MSVC-style world
- `x86_64-w64-mingw32-gcc` belongs to the MinGW or GNU-style world
- `clang` can belong to either world depending on its target

This matters because two tools may both run on Windows while still expect different flags,
different runtime libraries, or different default link behavior.

## Flags Depend on Both Tool and Target

Tool discovery and flag semantics are related but different concerns.

Examples:

- `cl` uses MSVC-style flags such as `/Fo`, `/Fe`, `/LD`, and `/link`
- GCC-like drivers use flags such as `-o`, `-c`, `-shared`, `-L`, and `-Wl,...`
- `clang` is usually invoked with GCC-like syntax, but a Clang target ending in `msvc` should not
  automatically receive GNU assumptions such as `-lm`

This is why Moon records both:

- the resolved compiler family
- the probed target triple

The `-lm` behavior belongs to this semantic layer, not just to executable lookup.

## Compile, Link, and Archive Usage

### Compile

`make_cc_command*` uses `cc_path` for compile steps (`-c` or `/c`) and backend-specific flags.

### Link

`make_linker_command*` also uses `cc_path` to drive linking:

- MSVC style: `cl ... /link ...`
- GCC or Clang style: `<cc> ... -o ...`

`-lm` is added only when the selected compiler is full-featured gcc-like and the probed target
triple does not contain `msvc`.

### Archive

`make_archiver_command*` uses resolved `ar_path`:

- `lib.exe` for MSVC
- `ar` or `llvm-ar` for gcc-like toolchains
- `tcc -ar` for TCC

## Maintenance Notes

When changing this area, it helps to keep two classes of bugs separate:

- "we picked the wrong tool"
- "we picked the right tool but emitted the wrong flags"

Those are different layers and should ideally be reviewed independently.
