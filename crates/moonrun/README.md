![check](https://github.com/moonbitlang/moonrun/actions/workflows/ci.yml/badge.svg)

# moonrun

Moonrun is the WebAssembly runtime for MoonBit, utilizing V8 at its core to offer an efficient and flexible environment for executing WASM.

## Platform Support

Moonrun currently supports 64-bit host targets only. 32-bit host targets are
not supported.

# Building and Running

## Building

To build the project, ensure that Rust and Cargo are installed. Then execute:
```
cargo build
```

## Running

To run a WebAssembly file:
```
./target/debug/moonrun path/to/your/file.wasm
```

## Rust library

Moonrun also exposes an experimental Rust library interface. It runs each Wasm
file with fresh per-run state and returns guest termination as data instead of
terminating the embedding process:

```rust
use moonrun::{Engine, RunOptions, RunOutcome};

let outcome = Engine::default()
    .run_file(
        "path/to/file.wasm",
        RunOptions::default()
            .with_args(["arg"])
            .environment([("APP_ENV", "test")]),
    )
    .unwrap();

match outcome {
    RunOutcome::Completed => {}
    RunOutcome::Exited(code) => eprintln!("guest exited with {code}"),
    RunOutcome::KilledBySignal(signal) => eprintln!("guest requested signal {signal}"),
}
```

An Engine can compile immutable Wasm Modules and execute them multiple times.
Runs are synchronous, so an embedding application owns any threads used for
concurrent execution.

```rust
let engine = Engine::default();
let module = engine.load_file("path/to/server.wasm").unwrap();
let outcome = engine.run(&module, RunOptions::default()).unwrap();
```

Embedders that already own the Wasm bytes can compile them directly and supply
the stable program name used for guest arguments and diagnostics:

```rust
let module = engine.compile("server.wasm", wasm_bytes).unwrap();
```

The current implementation remains V8-backed. Each run owns a private mutable
environment delta. When `environment` is omitted, the process environment is
its read-only backing source; Env reads it lazily as native `OsString` values.
The embedding process must not mutate that environment while any Run backed by
it is active. Full enumeration and inherited native spawn materialize the
source plus the Run's delta. Process stdio, working directory, and signal
compatibility behavior remain shared process dependencies. Compiled modules
reuse their prepared representation while each run creates fresh guest
execution state. Thread placement and lifecycle tracking remain the caller's
responsibility. Moonrun does not yet expose an interruption mechanism for a
running guest, and broader concurrent-run behavior is not yet part of the
interface contract.

## Memory Leak Reporting

When a program uses `moonbit:ffi/memory-sanitizer`, `moonrun` reports objects
that were registered but not freed before the program returned. A detected leak
is written to stderr with its allocation stack and makes `moonrun` exit
unsuccessfully.

## Experimental Policy

By default, running `moonrun` without `--policy` preserves existing behavior.

Supplying a JSON file with `--policy <path>` enables an experimental policy
system and switches supported moonrun-owned host surfaces into sandbox mode.
The policy is deny-by-default: omitted or empty `fs`, `net`, and `env` objects
deny that surface, and process spawning is disabled unless explicitly allowed.
Add entries only for the access the program should have. The policy covers
`moonbitlang/async` and moonrun's own `__moonbit_*_unstable` FFI surfaces. WASI
environment reads share the same per-Run environment and therefore observe the
`env` policy. WASI descriptors and preopens remain a separate capability
surface not controlled by the filesystem policy.

An empty JSON object denies all policy-covered filesystem, network, and
environment access:

```json
{}
```

To allow everything while still passing a policy file, use explicit wildcards:

```json
{
  "env": { "from_host": ["*"] },
  "fs": {
    "read": ["*"],
    "write": ["*"]
  },
  "net": {
    "dns": ["*"],
    "connect": ["*:*"],
    "bind": ["*:*"]
  },
  "process": { "spawn": true }
}
```

The simplest way to preserve legacy allow-all behavior is still to run without
`--policy`. The allow-all policy is useful when another tool requires a policy
file during migration or debugging.

The filesystem policy restricts native host paths. It does not create a virtual
guest filesystem, mount table, or portable `/` namespace. Relative filesystem
roots are resolved relative to the policy file. Guest relative paths are
resolved using the process current directory. Paths use the host platform's path
syntax; Windows policies may use normal Windows paths such as `C:\work` or
`C:/work`; JSON strings must escape backslashes as `C:\\work`. The filesystem
wildcard `"*"` allows every host path on every platform. List a root in both
`read` and `write` to allow read-write filesystem access.

The Run chooses its environment source; Moonrun Policy only selects and
overrides values from that source. Use `from_host` to select source variables
if present, `required_from_host` to require selected source variables, and
`env.set` for literal values. The CLI uses the host process environment;
library callers may provide a fixed source with `RunOptions::environment`.
`env.set` overrides selected values. Do not put secrets directly in the policy
file; pass them by name through `from_host` or `required_from_host`. Runtime
`set` and `unset` operations are immediately visible to subsequent moonrun-owned
and WASI environment reads without mutating the source. WASI preserves one
environment snapshot between a matching `environ_sizes_get` and `environ_get`
so the reported table size remains valid.

The temporary-directory base is resolved from the current Run environment. On
Windows, an unrestricted Run whose environment omits `TMP` and `TEMP` preserves
the native `GetTempPath2W` fallback; sandbox mode requires the temporary base to
be present in the selected environment and does not consult process-global
fallback variables.

A child that requests inheritance snapshots the current per-Run environment
when moonrun creates the spawn environment builder. Later parent `set` or
`unset` operations do not change that builder; the next spawn observes the
newer state.

Process spawning is disabled unless the request matches a `process.allow` rule
or `process.spawn` is `true`. Rules match the requested program exactly and
an optional `args_prefix` one complete argument at a time. Omitting
`args_prefix` allows any arguments for that program; an empty array is
equivalent. A non-empty prefix allows subsequent arguments. Multiple rules are
alternatives.

```json
{
  "process": {
    "allow": [
      { "program": "rustc" },
      { "program": "git", "args_prefix": ["status"] },
      { "program": "git", "args_prefix": ["diff", "--no-ext-diff"] }
    ]
  }
}
```

`process.spawn` and `process.allow` cannot be used together. Set
`process.spawn = true` only as a coarse allow-all escape hatch.

These rules authorize the logical request, not the executable file eventually
selected by the operating system. In particular, `PATH`, the working directory,
and the child environment can affect executable lookup and behavior. On Windows,
MoonBit appends `.exe` unless the requested program already ends in `.exe` or
`.com`; policy matching applies the same normalization and command-line escaping.
Allowing a shell, interpreter, package runner, or extensible tool can permit much
more than the visible argument prefix suggests.

A native child receives the Run environment snapshot but still has the host
user's ambient filesystem, network, and process access. The `fs` and `net`
objects do not sandbox child processes. PID-based process operations are
restricted to children spawned by the current moonrun instance while policy
mode is active. Native executable lookup also retains host-platform
compatibility semantics; it is not yet virtualized through the child Env.

```json
{
  "env": {
    "from_host": ["PATH", "SSL_CERT_FILE", "SSL_CERT_DIR"],
    "required_from_host": ["DEEPSEEK_API_KEY"],
    "set": {
      "APP_ENV": "prod",
      "API_BASE": "https://api.deepseek.com"
    }
  },
  "fs": {
    "read": ["allowed"],
    "write": ["scratch"]
  },
  "net": {
    "connect": [
      "api.deepseek.com:443",
      "hacker-news.firebaseio.com:443",
      "127.0.0.1:443",
      "[::1]:*"
    ],
    "bind": ["127.0.0.1:*"]
  }
}
```

To allow outbound access only to DeepSeek and Hacker News:

```json
{
  "net": {
    "connect": [
      "api.deepseek.com:443",
      "hacker-news.firebaseio.com:443"
    ]
  }
}
```

Hostname entries in `connect` allow DNS lookup for that host and allow
connections to the IP addresses returned by that lookup on the configured port.
Use `dns` only when a program needs standalone DNS lookup permission without
also granting outbound connects.

## Security Model and Known Limitations

Moonrun Policy authorizes operations performed by supported moonrun-owned host
interfaces. It is not an operating-system syscall sandbox or a complete
resource-isolation mechanism. In particular:

- SQLite supports private in-memory databases and file-backed databases through
  the default native VFS. Guest-selected database paths are authorized by
  Moonrun Policy when the connection is opened. Connections require read
  access to the database path and its parent directory; writable connections
  also require write access to that directory, where SQLite may use journal,
  WAL, and shared-memory files.
- File-backed SQLite authorization is currently a pathname check performed
  before the native VFS opens the database. If another guest filesystem
  operation or external actor replaces that path or one of its parent symlinks
  between those steps, SQLite can resolve a file outside the checked roots.
  Moonrun does not yet bind this authorization to a stable filesystem identity.
- SQLite may use its native VFS to create internal temporary files in the
  operating system's default temporary directory, even when the main database
  is in memory. These internal paths are not selected by the guest and do not
  currently pass through the filesystem policy.
- Native SQLite CPU time, heap usage, database size, and temporary-disk usage
  do not currently have per-run quotas.

Deployments that require strict isolation for untrusted workloads should also
use operating-system process isolation and resource limits. Security defects
that violate the documented policy should be reported privately rather than
added to this list with a public reproducer.

# Contribution

To contribute, please read the contribution guidelines at [docs/dev](./docs/dev/README.md).
