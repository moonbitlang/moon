# Behavior of `moonx`

> This page specifies the behavior of the `moonx` package and script runner.

## Scope

`moonx` executes either one standalone `.mbtx` file or exactly one main package
from the Mooncakes registry without installing it into the user's binary
directory. Other local paths, Git URLs, and wildcard package selectors are out
of scope.

`moonx` is another entrance to the `moon` executable, not a separately compiled
binary. The process selects the `moonx` command-line parser when its invoked
executable name is `moonx` or `moonx.exe`.

## Command line

```text
moonx [OPTIONS] <MBTX|PACKAGE> [PROGRAM_ARGS]...
```

The supported options are:

```text
--experimental-policy <PATH>
-v, --verbose
-h, --help
-V, --version
```

Everything after the `.mbtx` path or package coordinate, including
hyphen-prefixed values, is forwarded to the executed program. An explicit `--`
separator is accepted but is not required. `moonx` options therefore precede
the input.

The child process inherits the caller's working directory, environment,
standard streams, and signal behavior. `moonx` returns the child's exit code.

By default, `moonx` emits no informational output of its own. With `--verbose`,
registry or script dependency acquisition, build progress, and execution
details are written to stderr. Stdout remains reserved for the executed
program, whose standard streams are inherited unchanged.

When a policy-bearing `moonrun` directly spawns `moonx`, the host publishes the
canonical policy representation through an inherited OS handle and replaces a
reserved environment entry with the handle's process-local identifier. At
process entry, `moonx` takes ownership of the handle, removes the entry from its
ambient environment before registry or script dependency work, and makes the
handle inheritable again only for the final `moonrun` command. That `moonrun`
reads and closes the handle before constructing the Run environment. Detached
`moonx` processes therefore do not depend on the parent Run's lifetime, while
registry and build subprocesses and processes spawned by the resolved program
receive neither the marker nor the handle. No pathname crosses the process
boundary: Unix removes the temporary name before spawning, while Windows keeps
the delete-on-close file exclusively open. The guest therefore has no
filesystem name it can replace or modify.

Policy publication and handle setup run as part of the direct `moonx` spawn
job. If either fails, the job reports the corresponding OS error and no child
process is created or registered.

The inherited policy takes precedence over an explicit
`--experimental-policy`; the child may not replace or widen it. This applies to
both registry packages and standalone `.mbtx` files.

## Standalone `.mbtx`

An input whose extension is `.mbtx` is treated as a persistent standalone
script path. Moonx reuses the same synthesized-package dependency resolution,
build planning, and incremental target directory as a standalone linear-memory
Wasm build through `moon run`. Imports declared by `.mbtx` front matter or the
legacy prelude are resolved and built before the synthesized script package.
The resulting Wasm artifact then uses the same moonx execution path as a
registry Wasm artifact.

Standalone `.mbtx` execution uses the linear-memory Wasm backend.
Program arguments are forwarded to the final
`moonrun`, and `--experimental-policy` has the same meaning as for a registry
Wasm artifact.

A script may declare imports and an embedded Moonrun Policy in front matter
between two lines containing at least three hyphens, such as `----`. The front
matter uses the `moon.pkg` grammar, with `import { ... }` blocks and a single
`permission(...)` declaration:

```moonbit
----
import {
  "moonbitlang/core/env",
}
permission(
  env: { "from_host": ["PATH"] },
  fs: { "read": ["."] },
)
----

fn main {
  // ...
}
```

Moon's header parser accepts an optional shebang, such as `#!/usr/bin/env moonx`,
on the first line before front matter or a legacy import prelude. It also
accepts a leading UTF-8 BOM and blank lines after the shebang. Without a
shebang, front matter starts at the beginning of the file, optionally after a
BOM and whitespace. The closing delimiter is required; its hyphen count need
not match the opening delimiter. Imports retain versioned paths, named aliases,
and `*` import-all syntax. Multiple import blocks are
combined in source order. The old `import { ... }` prelude outside front matter
remains supported, including after a permission header; these imports follow
any front matter imports. Conditional and test-only imports remain unsupported.
Unknown declarations and duplicate `permission` declarations are errors.
The legacy `// policy:` YAML comment format remains supported for compatibility.
The format is detected from the source; no compatibility flag is required.
A script must not declare both `permission(...)` and a legacy `// policy:` block.

Shebang handling here applies only to Moon's header parsing and Moonrun's
policy loading. Moon passes the original script unchanged to compiler tools;
accepting a header does not guarantee that the compiler accepts the script.
The compiler warns about legacy imports outside front matter with
`unfenced_import` (warning 96).

The `.mbtx` path itself is passed to Moonrun as the policy source, so relative
filesystem roots are resolved from the script's directory. For `moon run -`
and `moon run -e`, Moon writes the source to a temporary `.mbtx` but retains the
invocation directory as the logical policy source directory. An explicit
`--experimental-policy` takes precedence over embedded policy. An inherited
policy remains host-owned and takes precedence over both. Front matter syntax is
validated during dependency resolution. Permission fields are decoded only when
the declaration is the effective policy for Wasm execution; overridden,
non-Wasm, and build-only paths do not validate the permission schema. Legacy
YAML is also parsed only when selected, so an explicit or inherited policy can
override malformed legacy YAML.

Moonx consumes an inherited policy handle before resolving `.mbtx`
dependencies and relays it only to the final `moonrun` process. Registry and
build subprocesses therefore cannot observe the reserved marker or inherit the
handle.

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

## Registry package execution

Registry packages run as linear-memory Wasm artifacts, using the same
acquisition and execution path as registry-backed `moon runwasm`:

1. Resolve the exact module version.
2. Compute the published linear-Wasm asset URL and cache path.
3. On a cache miss, download and verify the asset under a per-artifact lock.
4. Publish it atomically and execute it with `moonrun`.

In addition to HTTP request retries, response-body read failures restart the
checksum or Wasm download up to three times, discarding each incomplete body.

A missing published Wasm asset is an error. Version 1 does not fall back to
downloading source and building Wasm locally.

Registry-backed `moon runwasm` must use the linear Wasm backend consistently.
Tests for its cached-asset mode must use linear-Wasm fixtures rather than
WasmGC fixtures.

## Distribution

Cargo continues to build only the `moon` binary. Distributors create a second
filesystem entry named `moonx` containing the same executable bytes, using a
hard link where practical and a copy otherwise. Updating the external MoonBit
installer is required follow-up work outside this repository.
