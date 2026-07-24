# Go build-cache environment behavior

Research date: 2026-07-24.

Source baseline: the Go `master` branch at
[`bef396c3396fa7a31460cc4b892b90c5621e830f`](https://go.googlesource.com/go/+/bef396c3396fa7a31460cc4b892b90c5621e830f)
(committed 2026-07-22). All behavioral claims below refer to that revision.

## Conclusion

Go does **not** make build actions hermetic. Its build subprocesses inherit the
ambient process environment, while action IDs record selected build
configuration, tool identities, and declared compiler flags. Arbitrary
variables such as `CPATH` and `SDKROOT` are neither generally cleared nor
included in the action ID. `PWD` is not used as a free ambient input: Go
chooses each command's directory and constructs the child environment with the
corresponding `PWD`.

Therefore, matching Go's MVP shape does not require Moon to introduce an exact
or cleared environment. Moon should hash the selected compiler/archive tools,
their identities, explicit flags, target configuration, sources, and resolved
dependencies. It may defer arbitrary ambient-environment tracking, provided
that this is documented as the same non-hermetic limitation Go accepts.

## What enters Go action IDs

For an ordinary package compile action, Go records:

- the module Go version, `GOOS`, `GOARCH`, import/package configuration, build
  flags, source-file content hashes, and dependency content IDs;
- the applicable architecture setting (`GOARM`, `GOARM64`, `GO386`, `GOAMD64`,
  `GOMIPS`, `GOMIPS64`, `GOPPC64`, `GORISCV64`, or `GOWASM`);
- `GOEXPERIMENT`; and
- four explicitly recognized compiler-debug variables:
  `GOCLOBBERDEADHASH`, `GOSSAFUNC`, `GOSSADIR`, and `GOCOMPILEDEBUG`.

This is directly visible in
[`buildActionID`](https://github.com/golang/go/blob/bef396c3396fa7a31460cc4b892b90c5621e830f/src/cmd/go/internal/work/exec.go#L264-L381);
the architecture-variable mapping is in
[`GetArchEnv`](https://github.com/golang/go/blob/bef396c3396fa7a31460cc4b892b90c5621e830f/src/cmd/go/internal/cfg/cfg.go#L512-L537).
The source itself notes that the four compiler-debug variables are special
cases and contains a TODO about either preventing more such variables or
restricting subprocess environments.

For cgo packages, the same action ID also records:

- the cgo tool identity;
- the parsed `CC`, `CXX`, and `FC` command selections as applicable;
- the resulting `CGO_CPPFLAGS`, `CGO_CFLAGS`, `CGO_CXXFLAGS`,
  `CGO_FFLAGS`, and `CGO_LDFLAGS`, including source `#cgo` flags; and
- a probed identity for the selected C, C++, or Fortran compiler.

See
[`addCToolchainIDs`](https://github.com/golang/go/blob/bef396c3396fa7a31460cc4b892b90c5621e830f/src/cmd/go/internal/work/exec.go#L417-L453),
[`ccExe`/`cxxExe`/`fcExe`](https://github.com/golang/go/blob/bef396c3396fa7a31460cc4b892b90c5621e830f/src/cmd/go/internal/work/exec.go#L2585-L2598),
and
[`CFlags`/`buildFlags`](https://github.com/golang/go/blob/bef396c3396fa7a31460cc4b892b90c5621e830f/src/cmd/go/internal/work/exec.go#L2952-L2996).
The official cgo documentation likewise defines these variables as the
supported way to select compilers and supply flags:
[`cmd/cgo`](https://go.dev/src/cmd/cgo/doc.go#L68).

For a normal link action, Go records `GOOS`, `GOARCH`, build mode, the Go linker
identity and flags, the architecture setting, `GOEXPERIMENT`, `GOROOT`
(unless `-trimpath`), `GO_EXTLINK_ENABLED`, and dependency content/build IDs.
See
[`linkActionID` and `printLinkerConfig`](https://github.com/golang/go/blob/bef396c3396fa7a31460cc4b892b90c5621e830f/src/cmd/go/internal/work/exec.go#L1666-L1751).
That code still has a TODO asking whether further cgo or external-linker
settings need to be included, so Go itself does not claim a completely
hermetic external-link identity.

## Child environment and working directory

Go resolves the executable, sets `cmd.Dir` to the action directory, then uses
`cmd.Environ()` and appends action-specific overrides. Thus the subprocess
inherits the ambient environment rather than receiving a cleared allowlist.
The source comment explicitly says the `cmd.Environ()` call preallocates an
environment with the correct `PWD`:
[`Shell.runOut`](https://github.com/golang/go/blob/bef396c3396fa7a31460cc4b892b90c5621e830f/src/cmd/go/internal/work/shell.go#L584-L650).
C compiler actions add `TERM=dumb`; they do not replace the rest of the
environment:
[`cCompilerEnv`](https://github.com/golang/go/blob/bef396c3396fa7a31460cc4b892b90c5621e830f/src/cmd/go/internal/work/exec.go#L2315-L2320).
Even compiler capability probes inherit the environment and only append
`LC_ALL=C`:
[`gccSupportsFlag`](https://github.com/golang/go/blob/bef396c3396fa7a31460cc4b892b90c5621e830f/src/cmd/go/internal/work/exec.go#L2766-L2772).

The package directory may itself enter the build action ID when `-trimpath` is
off. Go omits or normalizes paths for GOROOT and temporary work-directory
inputs:
[`addPackageOrigin`](https://github.com/golang/go/blob/bef396c3396fa7a31460cc4b892b90c5621e830f/src/cmd/go/internal/work/exec.go#L384-L415).

## External compiler identity

Go first records the compiler command and flags in the package action ID. It
then probes a GCC-compatible driver with `-### -x <language> -c -`. For a
release compiler, the detected version line is the tool ID. For an
`experimental` compiler, Go reads the underlying compiler's build ID and
falls back to hashing that executable. See
[`gccToolIDPrefix`](https://github.com/golang/go/blob/bef396c3396fa7a31460cc4b892b90c5621e830f/src/cmd/go/internal/work/buildid.go#L201-L324).

Go separately caches this probe by the resolved compiler path and validates
the driver and underlying compiler using file metadata before reusing the
identity:
[`gccCompilerID`](https://github.com/golang/go/blob/bef396c3396fa7a31460cc4b892b90c5621e830f/src/cmd/go/internal/work/exec.go#L2803-L2875).
This deliberately treats release compilers with the same reported version as
equivalent; it is not always a byte hash of the driver binary.

## Arbitrary ambient variables

Direct evidence establishes both sides:

1. children inherit the ambient environment; and
2. the build/link action-ID functions enumerate selected values rather than
   hashing the complete environment.

Consequently, `CPATH` and `SDKROOT` do not enter the normal action ID merely
because they are set. They can still influence an inherited C compiler.
That potential under-invalidation is an inference from the two cited source
facts, not an explicit Go compatibility guarantee. The caller's original
`PWD` is also not hashed, but Go controls the child directory and supplies the
matching `PWD`; package-directory identity is handled separately as described
above.

The cgo documentation also warns that changes to files outside the package
directory are not detected automatically and recommends keeping non-Go source
inside the package directory:
[`cmd/cgo`](https://go.dev/src/cmd/cgo/doc.go#L99).
This is consistent with a declared-input cache rather than a fully hermetic
environment.

## Implications for Moon's standalone dependency cache

For the current standalone-script MVP:

1. Do not add an exact-environment execution mode solely to imitate Go.
   Preserve normal environment inheritance.
2. Keep the cache identity for native/LLVM actions focused on explicit,
   supported inputs: the resolved `CC`/archiver commands, tool identity,
   command arguments and explicit environment overrides, target configuration,
   source content, and resolved dependency graph.
3. Ensure the executable whose identity is recorded is the executable the
   action actually runs. Go resolves commands before execution and probes the
   selected compiler.
4. Keep working directories explicit and stable in the action model. This is
   separate from hashing every ambient variable.
5. Document `CPATH`, `SDKROOT`, compiler-wrapper-private variables, and similar
   ambient inputs as outside the MVP cache contract. If Moon later exposes
   supported native-build environment variables, add those variables
   deliberately to the action identity rather than hashing the entire process
   environment.

This means the previously proposed “clear all ambient environment for cached
native actions” correction can be dropped from the MVP. It is a possible
future hermeticity improvement, not a requirement imposed by Go's cache
design.
