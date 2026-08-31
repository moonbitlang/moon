# Behavior of `moonx`

> This page specifies the behavior of the `moonx` executable package runner.

## Scope

`moonx` resolves and executes exactly one main package from the Mooncakes
registry without installing it into the user's binary directory. Version 1
supports registry packages only; local paths, Git URLs, and wildcard package
selectors are out of scope.

`moonx` is another entrance to the `moon` executable, not a separately compiled
binary. The process selects the `moonx` command-line parser when its invoked
executable name is `moonx` or `moonx.exe`.

## Command line

```text
moonx [OPTIONS] <PACKAGE> [PROGRAM_ARGS]...
```

The supported options are:

```text
--target <wasm|native>        # defaults to wasm; native is deprecated
--experimental-policy <PATH> # wasm only
-v, --verbose
-h, --help
-V, --version
```

Everything after `PACKAGE`, including hyphen-prefixed values, is forwarded to
the executed program. An explicit `--` separator is accepted but is not
required. `moonx` options therefore precede the package coordinate.

The child process inherits the caller's working directory, environment,
standard streams, and signal behavior. `moonx` returns the child's exit code.

When a policy-bearing `moonrun` directly spawns `moonx`, the host publishes the
canonical policy representation through an inherited OS handle and replaces a
reserved environment entry with the handle's process-local identifier. At
process entry, `moonx` takes ownership of the handle, removes the entry from its
ambient environment before registry work, and makes the handle inheritable
again only for the final `moonrun` command. That `moonrun` reads and closes the
handle before constructing the Run environment. Detached `moonx` processes
therefore do not depend on the parent Run's lifetime, while registry
subprocesses and processes spawned by the resolved program receive neither the
marker nor the handle. No pathname crosses the process boundary: Unix removes
the temporary name before spawning, while Windows keeps the delete-on-close
file exclusively open. The guest therefore has no filesystem name it can
replace or modify.

Policy publication and handle setup run as part of the direct `moonx` spawn
job. If either fails, the job reports the corresponding OS error and no child
process is created or registered.

For the Wasm target, the inherited policy takes precedence over an explicit
`--experimental-policy`; the child may not replace or widen it. Native
execution does not pass through `moonrun`, so moonx closes an inherited policy
relay before registry work and otherwise preserves native execution behavior.

## Executable package coordinates

An Executable Package Coordinate selects exactly one main package. A module-only
coordinate selects its root package; a package suffix selects that exact package.
The selected package must be declared as a main package. `moonx` does not infer a
sole main package elsewhere in a module.

Pinned coordinates accept both existing forms:

```text
user/module/package@1.2.3
user/module@1.2.3/package
```

An explicit latest request accepts the corresponding forms:

```text
user/module/package@latest
user/module@latest/package
```

Documentation should prefer the first form. `@latest` refreshes the registry
index before resolving the newest version and fails if the refresh fails.
Unpinned coordinates resolve the latest version already known to the local
registry index. The index is updated only when the module cannot be resolved
locally, matching `moon runwasm`.

## Wasm target

The default `wasm` target means the linear-memory Wasm backend, not WasmGC.
It reuses registry-backed `moon runwasm` behavior:

1. Resolve the exact module version.
2. Compute the published linear-Wasm asset URL and cache path.
3. On a cache miss, download and verify the asset under a per-artifact lock.
4. Publish it atomically and execute it with `moonrun`.

A missing published Wasm asset is an error. Version 1 does not fall back to
downloading source and building Wasm locally.

Registry-backed `moon runwasm` must use the linear Wasm backend consistently.
Tests for its cached-asset mode must use linear-Wasm fixtures rather than
WasmGC fixtures.

## Native target

The `native` target is deprecated and scheduled for removal after September
14, 2026. Until then, it retains its existing behavior and emits a deprecation
warning on every invocation.

The `native` target reuses the registry acquisition, exact main-package
selection, and release build behavior of `moon install`, but publishes the
finished executable into the registry cache instead of the user's binary
directory.

Registry source acquisition never executes the downloaded module's
`scripts.postadd` hook. `moonx` stops after source acquisition and therefore
does not opt into the legacy hook. Existing project sync, `moon fetch`, and
binary-install paths retain an explicit postadd compatibility step.

By default, `moonx` emits no informational output of its own. With `--verbose`,
registry acquisition, build progress, and execution details are written to
stderr. Stdout remains reserved for the delegated program, whose standard
streams are inherited unchanged.

The Cached Executable Artifact is keyed by the resolved module version, package
path, and Target Backend. A cache hit executes it directly; Moon toolchain
upgrades do not invalidate an existing cached executable. Source trees and
build directories are temporary and are not retained.

Wasm and native artifacts share the coordinate-shaped registry asset cache:
`registry/cache/assets/<module>/<version>/<package>/<binary>`. Their file
suffixes distinguish `.wasm` and `.exe`; cached native artifacts use `.exe` on
every platform, including Unix. The physical path and lock scope are defined by
the [Moon home layout](./moon-home-layout.md).
The existing Mooncakes download cache may retain the verified source archive;
`moonx` does not retain an extracted source tree or build workspace alongside
the Cached Executable Artifact.

Native cache misses use the same concurrency and publication discipline as the
Wasm asset cache: check, lock, re-check, produce a temporary file, and publish
atomically. Failed downloads or builds leave no final cache entry.

## Distribution

Cargo continues to build only the `moon` binary. Distributors create a second
filesystem entry named `moonx` containing the same executable bytes, using a
hard link where practical and a copy otherwise. Updating the external MoonBit
installer is required follow-up work outside this repository.
