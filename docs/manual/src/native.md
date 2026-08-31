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
