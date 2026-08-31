# Native C Toolchain Resolution

This document explains how Moon currently resolves and uses the native C toolchain.

The resolved toolchain has three roles:

- compiler driver (`cc_path`)
- linker driver (also `cc_path`)
- archiver (`ar_path`)

After source selection, package overrides, and platform-specific selection are complete,
Moon resolves the effective toolchain's compiler and archiver through the shared
`moonutil::toolchain` executable resolver. The result contains concrete absolute executable paths.
Every effective native toolchain exposed to build lowering satisfies this invariant, so command
construction consumes the resolved paths without performing PATH lookup.

If either executable cannot be resolved, toolchain resolution fails immediately with an error that
identifies the missing compiler or archiver role. TCC uses the same resolved executable path for
both roles because archiving is performed with `tcc -ar`.

Moon does not resolve a standalone linker executable such as `ld` or `lld-link` in this path.
Linking is performed through the selected compiler driver.

`dsymutil` is a post-link debug-symbol tool, not a fourth C-toolchain role.
When planning an LLVM macOS debug build or an AArch64 Apple direct-object debug
build, Moon resolves `dsymutil` through the same shared executable resolver and
stores its concrete absolute path as planning metadata for the `GenerateDsym`
action. Other builds do not resolve it.

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
- the runtime implementation built from the C translation units under `lib/runtime/`
- the selected native allocator support object, when required
- package-level C stubs declared in `moon.pkg.json`

The high-level build flow is documented in `build.md`:

1. `BuildPackage`
2. `LinkCore`
3. `MakeExecutable`

For native backends, `LinkCore` emits:

- a generated C file for the generated-C native backend
- an object file for the LLVM backend
- an object file for a direct object native target

Package C stubs are handled separately:

1. each `stub.c` is compiled to an object file
2. all stub object files in the package are archived together
3. the final executable links against that per-package archive

The runtime uses the same explicit multi-step shape for ordinary native builds:

1. each shipped `lib/runtime/*.c` translation unit is compiled independently
2. for release builds, supported prebuilt SIMDUTF objects are enabled and
   archived with the runtime objects into one static library
3. the final executable links against that runtime library

`MOONBIT_ALLOCATOR` may override the native runtime allocator:

- unset on Linux and macOS with a non-MSVC, non-TCC compiler that was not selected through
  `MOON_CC`: compile the runtime with
  `-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC` and link `libmoonbitrun.o`
- unset on other native toolchains: compile the runtime with
  `-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_SYSTEM` and do not link `libmoonbitrun.o`
- `mimalloc`: compile the runtime with
  `-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC` and link `libmoonbitrun.o`;
  fail during planning when that support object is unavailable for the selected
  platform or toolchain, including Windows and TCC
- `system`: compile the runtime with
  `-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_SYSTEM` and do not link `libmoonbitrun.o`

The `MOONBIT_ALLOCATOR` macro is a runtime compile setting. Moon passes it only
when compiling the shipped runtime sources, not when compiling package C stubs
or the C file emitted by `moonc link-core`.

During the toolchain transition, Moon falls back to the legacy `lib/runtime.c`
when the split runtime directory is absent.

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
- If `MOON_AR` is set together with `MOON_CC`, Moon passes it into compiler resolution for the
  regular native pipeline.
- For `cc`, `gcc`, and `clang`, that means the resolved archiver path comes from `MOON_AR`.
- For `cl`, Moon still resolves `lib.exe` next to the compiler, so `MOON_AR` is ignored.
- For `tcc`, Moon still uses `tcc -ar`, so `MOON_AR` is ignored.
- `MOON_CC` takes precedence over package-level compiler overrides.

This override is global to the current Moon invocation.

## Package-Level Override

`moon.pkg.json` may specify native compiler overrides:

- `link.native.cc`
- `link.native.stub_cc`

These are parsed into `CC` values during build-plan construction.

They are useful as escape hatches, but they are toolchain-specific, not portable configuration.
For example, `cc = "cl"` is MSVC-specific, while `cc = "gcc"` or GNU-style flags are specific to
other toolchain families.

## Default Auto-Detection

When `MOON_CC` is unset and no package-specific override is being applied to that step, Moon probes
for a default native toolchain.

On Windows, Moon first tries to discover an MSVC toolchain environment for the current 64-bit host
architecture using the Visual Studio/MSVC discovery logic. The discovery target is
`x86_64-pc-windows-msvc` on x64 Windows hosts and `aarch64-pc-windows-msvc` on ARM64 Windows hosts.
Moon does not model cross compilation in this path. This discovery is needed so generated-C builds
can use `cl.exe` outside a Developer Command Prompt. If MSVC discovery fails, or on non-Windows
hosts, Moon falls back to PATH probing in this order:

1. `cl`
2. `cc`
3. `gcc`
4. `clang`

Moon does not fall back to a bundled compiler. If no system toolchain is
available, planning fails with the tool-resolution error.

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

### MSVC (`cl` and `clang-cl`)

- Archiver is `lib.exe`. A path-like `clang-cl` installation falls back to its sibling
  `llvm-lib.exe` when `lib.exe` is absent.
- For a bare MSVC override such as `cl.exe`, Moon uses the discovered full paths for both
  `cl.exe` and `lib.exe`. The discovered command environment is attached separately and is not
  relied on to locate either executable.
- A path-like MSVC override uses its sibling librarian (`lib.exe`, or `llvm-lib.exe` for
  `clang-cl` when `lib.exe` is absent) and does not inherit the environment from a separately
  discovered toolchain. The configured environment must already support that toolchain.

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

For a plain Clang driver that targets the MSVC ABI, Moon prefers the compiler-reported
`llvm-lib` when it is available. This keeps Clang's GCC-style compiler-driver syntax separate from
the MSVC-style librarian syntax.

### Apple targets

On macOS, a compiler that reports an Apple Darwin target uses the active Apple `libtool` reported
by `xcrun --find libtool`. Both Xcode and the Command Line Tools provide this tool. An explicit
`MOON_AR` remains an override for `cc`, `gcc`, and plain `clang`.

## Toolchain Families and Compatibility

The main compatibility boundary is not simply "same operating system" or "same machine".
The relevant boundary is the target toolchain family and ABI.

In practice, the important dimensions are:

- target architecture
- object format
- ABI and runtime ecosystem
- command-line style of the selected tool driver
- required command environment
- CRT policy on MSVC

Examples:

- `cl` and `clang-cl` belong to the MSVC-style world
- `x86_64-w64-mingw32-gcc` belongs to the MinGW or GNU-style world
- `clang` can belong to either world depending on its target

This matters because two tools may both run on Windows while still expect different flags,
different runtime libraries, or different default link behavior.

Moon keeps MSVC discovery environment values such as `PATH`, `INCLUDE`, `LIB`, and `LIBPATH` as
command environment. These values are not equivalent to simply appending more `/I` or `/LIBPATH`
flags: the driver and SDK tools use the full environment contract.

Moon currently uses focused predicates for the compatibility checks it needs, rather than a general
"native build contract" that validates every C compiler involved in a build. This keeps fake
toolchains, dry runs, and package-level escape hatches working unless a concrete build step requires
a stricter tool.

The generated-C native backend does not require MSVC. If a user explicitly sets `MOON_CC` or
`link.native.cc`, Moon keeps using that compiler for the generated-C path. Package-level
`link.native.stub_cc` is resolved independently for that package's C stubs. Moon does not preflight
whether an independently configured stub compiler is link-compatible with the executable compiler;
incompatible objects or archives fail naturally when the final linker consumes them.

Package-level `link.native.cc-flags` apply when compiling the C file emitted by `moonc link-core`.
If any selected executable package sets these flags, Moon uses the generated-C native backend
instead of direct object output so the configured flags are not skipped.
This native payload form is currently selected once per invocation, so one such package makes every
selected executable in that invocation use generated C. This scope also keeps the invocation-wide
runtime product and package-wide C-stub products on one compatible native toolchain and realization.
Selecting the payload form per executable requires those shared products to be keyed by their
effective native toolchain and realization; changing only `LinkCore` and `MakeExecutable` would
allow incompatible shared artifacts in mixed-mode builds.

The direct object native target `x86_64-pc-windows-msvc` is stricter: it requires selecting a
`cl`-compatible compiler driver and preserving the discovered Visual Studio command environment.

The MSVC CRT policy is a separate axis from the ABI family. Moon currently forces the static CRT
flag `/MT` for `cl`-style compile commands, matching the default MSVC runtime library list that uses
`libcmt.lib`. `/MD` is not currently modeled as a selectable policy. `/LD` is not a CRT policy; it
is emitted when the requested output type is a shared library.

## Flags Depend on Both Tool and Target

Tool discovery and flag semantics are related but different concerns.

Examples:

- `cl` uses MSVC-style flags such as `/Fo`, `/Fe`, `/LD`, and `/link`
- GCC-like drivers use flags such as `-o`, `-c`, `-shared`, `-L`, and `-Wl,...`
- `clang` is usually invoked with GCC-like syntax, but a Clang target ending in `msvc` should not
  automatically receive GNU assumptions such as `-lm`
- TCC on macOS may need an explicit active SDK library path because its built-in SDK search paths
  can point at the standalone Command Line Tools SDK instead of the selected Xcode SDK.

This is why Moon records both:

- the resolved compiler family
- the probed target triple

The `-lm` behavior belongs to this semantic layer, not just to executable lookup.

## Compile, Link, and Archive Usage

### Compile

`make_cc_command*` uses `cc_path` for compile steps (`-c` or `/c`) and backend-specific flags.
For TCC on macOS, Moon also passes the active SDK `usr/lib` path reported by
`xcrun --sdk macosx --show-sdk-path` when it resolves to an existing directory.
If `xcrun` is missing or returns an unusable path, Moon omits the extra `-L` flag instead of
failing command construction.

### Link

`make_linker_command*` also uses `cc_path` to drive linking:

- MSVC style: `cl ... /link ...`
- GCC or Clang style: `<cc> ... -o ...`

`-lm` is added only when the selected compiler is full-featured gcc-like and the probed target
triple does not contain `msvc`.

### Archive

`make_archiver_command*` uses resolved `ar_path`:

- `lib.exe` or `llvm-lib` for MSVC-style librarians
- `libtool -static` for Apple Darwin targets on macOS
- `ar` or `llvm-ar` for remaining gcc-like toolchains
- `tcc -ar` for TCC

`libtool`, `lib.exe`, and `llvm-lib` create an archive from the complete member list passed by
Moon. GNU- and LLVM-style `ar rcs`, and TCC's equivalent archive mode, update an existing archive
and can retain members that are no longer present in the input list. Moon classifies this behavior
from the resolved `ARKind`, not from the host OS.

For update-style archivers, runtime and package C-stub static archive output paths include a
fingerprint of the ordered logical member identities. The fingerprint is computed once during
build planning. File contents are not hashed; they remain ordinary n2 inputs, so content changes
update the same archive while membership changes select a fresh path. Apple libtool and MSVC-style
librarians keep stable output paths because they recreate the archive from the complete input
list. Build lowering invokes the resolved librarian directly in both cases.

### Generate macOS debug symbols

For LLVM macOS debug builds and AArch64 Apple direct-object debug builds, Moon
models debug-symbol generation as a separate `GenerateDsym` action:

1. `MakeExecutable` invokes the resolved compiler driver and produces the executable.
2. `GenerateDsym` depends on that executable, invokes the resolved `dsymutil`,
   and produces `<executable>.dSYM`.

The commands are not joined with `&&`. This keeps the linker invocation as
structured argv, makes the executable-to-dSYM dependency explicit, and records
the `dsymutil` executable as an n2 file input.

## Maintenance Notes

When changing this area, it helps to keep two classes of bugs separate:

- "we picked the wrong tool"
- "we picked the right tool but emitted the wrong flags"

Those are different layers and should ideally be reviewed independently.
