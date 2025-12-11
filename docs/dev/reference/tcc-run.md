# TCC Run Mode

> This document reflects the state of the repository around 2025.12.
> Please use the actual implementation as the ultimate source of truth.
>
> A major portion of this document is written by an LLM.

Moon can execute the artifacts of the native (C) backend directly through `tcc -run`
in order to reduce build times.
This document describes the TCC run mode,
which keeps the regular MoonBit compilation stages
but reshapes the native toolchain steps
so that linking happens at execution time instead of during the build.

## When TCC run mode is selected

TCC run mode is considered only in debug builds that target the native backend.
The planner verifies three conditions before switching to the TCC run flow:

- The invocation runs on Linux or macOS, where the bundled `tcc` is available.
- No package in the build graph requests custom native compilers or flags.
  If any package opts into its own toolchain, the run falls back to the regular native pipeline.
- The requested action actually needs a runnable binary (for example, `run` or `test`).
  Pure build or check requests keep using the standard native backend.

If every gate passes, the planner substitutes the backend with the dedicated TCC run backend.
Otherwise, nothing changes and the regular linker path is used.

## How the pipeline diverges

TCC run mode keeps the logical build steps described in the [architecture][] and [build][] references:

[architecture]: ./arch.md
[build]: ./build.md

1. `BuildPackage` compiles MoonBit sources into CoreIR and emits package interfaces.
2. `LinkCore` gathers all transitive dependencies
   to produce a single C artifact for the selected target kind.
3. Native-specific nodes prepare artifacts for execution.

The first two stages behave identically across all native backends.
Divergence happens in the native-specific step:

- The runtime code is compiled into a shared library instead of an object file.
- Each package's C stubs remain as object files, but are collected into shared libraries (`.so`/`.dylib`).
- The final `MakeExecutable` node no longer compiles and link the resulting C code.
  Instead, it writes a response file that captures the full `tcc -run` command line,
  pointing at the C artifact and the shared libraries prepared above.

### Notes

We emit a shared library instead of a static library or plain object file because TCC,
on \*nix systems, can only consume ELF.
On macOS the object files are Mach-O,
while the shared libraries/executables we care about are ELF,
so using a shared library in place of an object file lets TCC link against them successfully.

We use a response file so that all concrete build and link details stay inside the build-graph generator.
The executable runners only see "call `tcc` with `@<response-file>`",
which avoids leaking flags/paths out of the pipeline
and keeps the runners loosely coupled to the build system.

## Execution at runtime

When `moon` later executes the target,
it detects the TCC run backend and launches the internal `TCC` with the response file recorded earlier,
`tcc @<response-file> [args...]`.

This behavior is transparent to the user and the rest of the build system.
The command and user-provided arguments are generated in the exact same way as other backends.

## Fallback behavior

Any violation of the eligibility rules reverts the pipeline to the regular native backend. This applies per invocation rather than per package: once one package blocks TCC run mode, every target in that run uses the traditional linker path so the build graph stays coherent.
