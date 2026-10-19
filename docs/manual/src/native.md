# Native build configuration

## Backend selection

For `--target native`, debug builds on macOS AArch64, Linux x86_64, and Windows
x86_64 use the new native backend by default, with `moonc` emitting object files
directly. Set `MOONBIT_NEW_NATIVE=0` to use generated C instead; other values keep
the default. Unsupported hosts and release builds use generated C even when
`MOONBIT_NEW_NATIVE=1` is set. If a selected executable package configures
`link.native.cc-flags`, the invocation also uses generated C.

On Windows, the new native backend requires a cl-compatible MSVC toolchain,
such as `cl.exe` or `clang-cl.exe`, to compile the runtime and C stubs and link
the executable.

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
Windows and with TCC.

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
