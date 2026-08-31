# Legacy TCC Run Compatibility

> This document describes dormant compatibility code. TCC run mode is not
> selectable behavior.

## Current behavior

Moon no longer selects TCC run mode. When direct native object production is
disabled or unavailable, Moon selects the Generated-C Native Backend and
resolves an available system C toolchain. If no system toolchain is available,
planning reports the normal tool-resolution error.

In particular, setting `MOONBIT_NEW_NATIVE=0` does not enable TCC. Native
`run` and `test` actions produce ordinary executables through the generated-C
pipeline.

## Retained compatibility code

The build model, lowering code, and runtime launcher no longer contain a TCC-run
mode. A hidden response-file writer and dormant response-file and shared-library
layout helpers remain while their removal is staged separately. The CLI and
build graph cannot reach that compatibility code. Explicitly selecting an
external TCC as the regular C compiler remains supported and is independent of
the removed mode.

Historically, eligible Linux and macOS debug `run` and `test` invocations used
the following realization:

1. `BuildPackage` compiled MoonBit sources into CoreIR and emitted package
   interfaces.
2. `LinkCore` gathered transitive dependencies and emitted generated C.
3. The runtime was built as a shared library, and package C stubs were collected
   into shared libraries.
4. `MakeExecutable` wrote a response file containing the `tcc -run` command
   line instead of linking an executable.
5. The runtime launcher invoked the bundled TCC with that response file and the
   program arguments.

The remaining response-file and shared-library helpers do not describe current
native command behavior.
