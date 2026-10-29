# Native build configuration

## Backend selection

For `--target native`, debug builds on macOS AArch64, Linux x86_64, and Windows
x86_64 use the new native backend by default, with `moonc` emitting object files
directly. Set `MOONBIT_NEW_NATIVE=0` to use generated C instead; other values keep
the default. Unsupported hosts and release builds use generated C even when
`MOONBIT_NEW_NATIVE=1` is set. If a selected executable package configures
`link.native.cc-flags`, or `MOON_CC` selects a compiler for another target, the
invocation also uses generated C.

On Windows, the new native backend requires a cl-compatible MSVC toolchain,
such as `cl.exe` or `clang-cl.exe`, to compile the runtime and C stubs and link
the executable.

## C compiler caching

Set `MOON_CC_WRAPPER` to use a compiler cache such as ccache or sccache:

```sh
MOON_CC_WRAPPER=ccache moon build --target native --release
MOON_CC=clang MOON_CC_WRAPPER=sccache moon test --target native
```

The value is one executable name or path, including paths containing spaces;
it is not a shell command and cannot include arguments. Unset it or set it to
an empty string to disable the wrapper. Custom wrappers receive the selected
compiler path followed by its arguments. Configure wrapper options through
its environment or a wrapper script.

Moon uses the wrapper for generated C, runtime C, and C stubs, including those
in foreign libraries and LLVM builds. Each source is compiled separately to
an object before linking or archiving. Compiler discovery, linking, and
archiving run without the wrapper. The wrapper does not change compiler
selection, backend selection, or the default allocator.

`MOON_CC` remains a compiler executable name or path; use `MOON_CC_WRAPPER`
instead of setting `MOON_CC="ccache clang"`. Cache reuse across different
directories also depends on the cache tool's path and debug-info settings.

## Floating-point arithmetic

Native C compilation with GCC/Clang disables floating-point contraction by
default with `-ffp-contract=off`. This preserves separate rounding of operations
such as multiplication followed by addition. MSVC-compatible compilers retain
their default behavior.

## Allocator

`MOONBIT_ALLOCATOR` selects the allocator used by native runtime builds:

- `mimalloc` compiles the runtime for mimalloc and links the shipped
  `libmoonbitrun.o` support object.
- `system` compiles the runtime for the system allocator and does not link
  `libmoonbitrun.o`.

The selected value is defined both when compiling the shipped runtime sources
and when compiling the C emitted by `moonc link-core`, so the program inlines
the same allocator as the runtime it links against.

When the variable is unset, Moon preserves the platform and toolchain default.
Selecting `mimalloc` fails when its support object is unavailable, including on
Windows, with TCC, and when cross-compiling.

## Cross-compiling

Set `MOON_CC` (and `MOON_AR` if needed) to a compiler for another target, for
example a wrapper script that runs `zig cc -target x86_64-linux-musl "$@"`.
Moon runs the compiler with `-dumpmachine` to find its target. When the
architecture, operating system, or ABI (for example `musl` on a glibc Linux
host) differs from the host, Moon does not link the
prebuilt objects in the MoonBit lib directory (`libmoonbitrun.o`, the SIMDUTF
objects, and `libbacktrace.a`), because they are built for the host. The
runtime then uses the system allocator and its scalar UTF code path, and
panics do not print a backtrace.

Cross-compiling is best effort and works only through the global `MOON_CC`.
A package-level `cc` does not change the target of the shared runtime or of the
C stubs of other packages, so it cannot be used to cross-compile. The
`platform` cfg and the prebuilt `core` library still follow the host.

## Reference cycle collection

Set `MOON_COLLECT_REF_CYCLE=1` to enable experimental reference cycle collection:

```sh
MOON_COLLECT_REF_CYCLE=1 moon run main --target native
MOON_COLLECT_REF_CYCLE=1 moon test --target wasm
```

It is off by default. Unset the variable or set it to `0` to disable it;
only the value `1` enables it.

The setting applies to `build`, `run`, `test`, and `bench`, including standalone
files, in both debug and release builds. Native builds support both generated C
and the machine backend (direct object output), with either native allocator.
Enabling collection preserves the usual backend selection, including
`MOONBIT_NEW_NATIVE`. Machine builds require a compiler with trial-deletion
support; Wasm builds require one supporting `-enable-trial-deletion`.
The setting has no effect on WasmGC, JavaScript, or LLVM.
