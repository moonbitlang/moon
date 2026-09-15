# Native build configuration

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
files, in both debug and release builds. Native builds use generated C while it
is enabled, including when `MOONBIT_NEW_NATIVE=1` is set. Both native allocators
are supported. Wasm builds require a compiler supporting `-enable-trial-deletion`.
The setting has no effect on WasmGC, JavaScript, or LLVM.
