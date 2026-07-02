# One Native Build Contract Per Invocation

Moon resolves one native build contract for each native build invocation. The contract contains the ABI family, CRT linkage policy where applicable, and toolchain environment. Runtime compilation, generated-C compilation and linking, package C stub archives, and direct-object executable linking must all match that contract.

When Moon targets the MSVC ABI on Windows, the native toolchain is not just a `cl.exe`-compatible executable path: the compiler, linker, headers, libraries, Windows SDK, and CRT library set are tied to one Visual Studio/MSVC environment. Moon resolves one MSVC-family toolchain environment per native build invocation and reuses it for runtime compilation, package C stub compilation, and final executable linking, regardless of whether MoonBit produced generated C or direct object code.

The implementation separates the durable compatibility choice from the executable used for an individual command:

- `NativeBuildContract` is the invocation-level ABI/CRT/environment contract and is the only native toolchain state stored on the build plan as a whole.
- `NativeCommandDriver` is the concrete compiler/linker/archiver selected for one native action.
- `NativeToolchain` pairs a command driver with the contract it has been validated against before lowering emits command lines.

Package-level `link.native.cc` and `link.native.stub_cc` therefore produce action-scoped command drivers. They may choose a compatible executable, but they do not choose an independent ABI family, CRT linkage, or MSVC environment.

## Considered Options

- Resolve tools independently for each package or native build step. Rejected because a single final executable could silently mix different Visual Studio installations, Windows SDK views, ABI families, or CRT policies.
- Scan every package configuration first and let package-level overrides choose the global toolchain. Rejected because package overrides are escape hatches for specific build steps; they should be validated against the invocation contract, not decide the executable's ABI world.
- Build shared packages multiple times when roots require different native contracts. Rejected for now because it requires action keys, artifact paths, and cache keys to include the native contract for runtime objects, C stub archives, generated C outputs, and final executable links.
- Keep MSVC discovery only in the direct-object backend path. Rejected because generated-C builds that use `cl.exe` or `clang-cl.exe` have the same MSVC environment and CRT constraints.

## Consequences

- `MOON_CC` remains the highest-precedence user override, but when a build path requires the MSVC ABI it must name a cl-compatible driver such as `cl.exe` or `clang-cl.exe`.
- Package-level `link.native.cc` and `link.native.stub_cc` may choose a compatible driver, but must not switch the invocation to a different ABI family, CRT policy, or independent MSVC environment.
- A native build invocation rejects conflicting package overrides instead of building shared packages twice under different native contracts.
- Windows default resolution should use MSVC environment discovery when the MSVC ABI is required or selected, and generic `cc`/`gcc`/`clang` fallback should only apply outside that MSVC-required path.
- `clang-cl.exe` is treated as MSVC-family for ABI and environment purposes even though it is a Clang driver.

## References

- [Use the Microsoft C++ Build Tools from the command line](https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line?view=msvc-170)
- [CL environment variables](https://learn.microsoft.com/en-us/cpp/build/reference/cl-environment-variables?view=msvc-170)
- [/MD, /MT, /LD runtime library options](https://learn.microsoft.com/en-us/cpp/build/reference/md-mt-ld-use-run-time-library?view=msvc-170)
- [clang-cl user manual](https://clang.llvm.org/docs/UsersManual.html#clang-cl)
