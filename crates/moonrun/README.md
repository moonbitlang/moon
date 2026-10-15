![check](https://github.com/moonbitlang/moonrun/actions/workflows/ci.yml/badge.svg)

# moonrun

Moonrun is the WebAssembly runtime for MoonBit. Shipping builds use V8, while a
Wasmtime 48 backend is available as a compile-time development feature.

## Platform Support

Moonrun currently supports 64-bit host targets only. 32-bit host targets are
not supported.

# Building and Running

## Building

To build the project, ensure that Rust and Cargo are installed. Then execute:
```
cargo build
```

The default `moonrun` build selects V8. To build the Wasmtime backend:

```sh
cargo build -p moonrun --no-default-features --features wasmtime
```

V8 remains selected when both engine features are enabled; select Wasmtime by
disabling default features as shown above. Both engines use the same Moonrun
Host Domains for async operations, filesystem and environment access, SQLite,
memory-sanitizer state, policy, and termination. Wasmtime supports both `wasm`
and `wasm-gc`, including JS-string builtins with `_` imported string constants,
the MoonBit test driver, and the same Moonrun-owned WASIp1 capability surface
as V8. WASIp1 descriptors and preopens remain separate from Moonrun Policy;
both engine adapters invoke the same shared WASIp1 host operations.

## Experimental wee8 backend

The opt-in `wee8` backend uses V8's Wasm C entry points and the same public
Rust `Engine`, `Module`, and `RunOptions` interface. Wasmtime continues to use its
native Rust interface. Moonrun depends on the upstream `wee8` Rust crate at a
pinned Git revision, like `n2`. That crate owns the safe native handles, private C
declarations, artifact acquisition, and linking. Moonrun owns the engine adapter
and shared host services.

Build it directly with Cargo:

```sh
cargo build -p moonrun --no-default-features --features wee8
```

The upstream crate's build script downloads the target's archive and matching header from the
pinned [V8 release](https://github.com/moonbitlang/v8-custom-builds/releases/tag/v0.106.0-moonbit.1),
checks SHA-256 before extraction, and caches the downloads in Cargo's build
output directory. It uses Cargo's target, including for cross-compilation.
Only the five published Linux/macOS/Windows targets have automatic downloads.
Set `WEE8_MIRROR` to an alternate release-download base URL if needed; the
script appends the pinned release tag and asset names.

CI exercises this download path and tests wee8 separately on Linux, macOS and
Windows. The existing V8 backend and default build continue using upstream
rusty_v8. Building wee8 requires libclang for generating its C declarations.

For a source or offline build, use the [pinned recipe](https://github.com/moonbitlang/v8-custom-builds)
and provide both its decompressed archive and matching header explicitly to
bypass downloading:

```sh
WEE8_ARCHIVE=/path/to/libwee8.a WEE8_HEADER=/path/to/wasm_c_api_<target>.h \
  cargo build -p moonrun --no-default-features --features wee8
```

Decompress the target's archive first (`wee8.lib` on Windows). The header is
checked against the supported revision; stock V8's older same-named functions
have an incompatible ABI. The supplied archive must come from the matching
recipe. This backend cannot be linked alongside rusty_v8 in the same binary.
Enabling both `wee8` and `wasmtime` is allowed and selects wee8; Wasmtime uses
its native Rust API and does not introduce another V8 archive.

This is a working integration MVP, not a replacement for either full backend:

- Wasm 1.0 numeric execution, bulk-memory instructions, multi-value guest code,
  guest-defined memory/globals/tables, and `_start`. Bulk memory and multi-value
  loops are needed by MoonBit's `wasm` compiler output; the Rust host-call
  interface still accepts only numeric values and at most one return value.
- WASIp1 `fd_write`, `proc_exit`, `args_sizes_get`, and `args_get`, using Moonrun's
  existing host operations. These implementations remain defined for all engines;
  only registration is limited to these four imports in the wee8 adapter.
  Unsupported imports and signatures fail explicitly.
- Reusable modules and fresh state per run. Compiled modules cross stores using
  V8's private serialization; this is not a persistent, portable AOT artifact.
- Host state remains in Rust. Guest memory is borrowed only during a synchronous
  host call, using its current allocation after any guest memory growth.

Host-imported memory/globals/tables, guest re-entry from a host callback,
WasmGC/reference values, exception tags, the MoonBit test driver, custom stack
limits, and debugger access are not exposed yet. Memory-dependent imports from
a Wasm start function are rejected because this C API provides no caller-instance
lookup before instantiation returns. V8 still uses JS machinery internally.
The complete backend conformance suite is not claimed to pass; CI runs the
focused wee8 tests instead.

Run the focused integration checks:

```sh
cargo test -p moonrun --no-default-features --features wee8 --test wee8
cargo test -p moonrun --no-default-features --features wee8 --lib
```

The integration tests compile a MoonBit program for `wasm` in both debug and
release modes and run it through the wee8 CLI, checking Unicode output and
arithmetic. This requires the MoonBit toolchain, as do the other Moonrun
compiler integration tests. It does not establish support for all MoonBit host
imports or the `wasm-gc` backend.

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
use moonrun::{Engine, RunOptions, RunOutcome, WorkingDirectory};

let outcome = Engine::default()
    .run_file(
        "path/to/file.wasm",
        RunOptions::default()
            .with_args(["arg"])
            .with_working_directory(WorkingDirectory::Ambient),
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

The shipping configuration remains V8-backed; the Wasmtime feature provides
the same library surface at compile time. Both configurations inherit process
stdio, environment, and working-directory behavior. The library does not
install an operating-system signal handler. Callers may create a
`signal_channel`, pass its receiver to `Engine::run_with_signal_receiver`, and
send signals to that Run after its guest async handler is ready. This is
cooperative delivery only: signals are not retained before registration and
cannot yet forcibly interrupt guest execution. The `moonrun` CLI owns a
process-lifetime adapter and forwards accepted signals directly through such a
channel. On Unix it blocks its managed signals before creating the Engine and
receives them on a `sigwait` thread; spawned guest processes retain the signal
mask from before that block. The guest starts and stops delivery directly to its
Run's completion queue, so signal handling does not consume an async worker.
Older Wasm guests retain a virtual signal-wait Job and its cancellation hook.
Ordinary Unix Jobs retain the thread pool's `pthread_kill(SIGUSR2)` path.
If the Run does not accept a signal, the adapter applies the process's existing
signal behavior.

A Run can make its working-directory selection explicit with
`WorkingDirectory::Ambient`, which is also the default and preserves the
existing process-global behavior. `WorkingDirectory::Fixed(path)` resolves
relative paths against a supplied directory without changing process cwd. Compiled modules reuse their prepared representation
while each run currently creates a fresh Runtime that owns its environment,
policy, working-directory selection, and domain state alongside fresh guest
execution state. Thread placement and lifecycle tracking remain the caller's
responsibility.

### Fixed working directory

```rust
use moonrun::{Engine, RunOptions, WorkingDirectory};

let engine = Engine::default();
let module = engine.load_file("agent.wasm")?;
let options = RunOptions::default()
    .with_working_directory(WorkingDirectory::Fixed("workspace".into()));
let outcome = engine.run(&module, options)?;
```

The directory must exist and is canonicalized when execution begins. Relative
filesystem paths, SQLite filenames, and child directories use this root.
Absolute paths and explicit empty filenames retain their existing meaning.
This selects path interpretation; it does not grant access, create a sandbox,
or change policy-denial errors. Standard streams remain ambient process I/O.

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
`moonbitlang/async` and moonrun's own non-WASI `__moonbit_*_unstable` FFI
surfaces. WASI operations generally remain outside Moonrun Policy
(`wasi_snapshot_preview1` / `__moonbit_wasi_unstable`), with one shared-state
exception: `environ_get` and `environ_sizes_get` expose the Runtime environment
realized from the `env` policy. WASI descriptors, preopens, and other WASI
operations are still configured separately from Moonrun Policy.

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
resolved using the Runtime Working Directory; its only current mode observes the
process current directory at policy-check time. Paths use the host platform's
path syntax; Windows policies may use normal Windows paths such as `C:\work` or
`C:/work`; JSON strings must escape backslashes as `C:\\work`. The filesystem
wildcard `"*"` allows every host path on every platform. List a root in both
`read` and `write` to allow read-write filesystem access.

The `env` section provisions the guest environment. Use `from_host` to copy
selected host variables if present, `required_from_host` to require selected
host variables, and `env.set` for literal values. `env.set` overrides values
copied from the host. The realized environment is shared by MoonBit runtime
environment APIs and WASI environment calls. Do not put secrets directly in the
policy file; pass them by name through `from_host` or `required_from_host`.

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

As an experimental feature, standalone `.mbtx` scripts can carry the same
policy inline as YAML; this format may change. Put a leading `// policy:` block
at the start of the script and prefix every YAML line with `//`, without `---`
delimiters. `moon run` and `moonx` detect this block and use the script itself
as the policy source. Explicit command-line policy paths override the embedded
policy; inherited policies cannot be replaced by either. An embedded block is
parsed only when it is the effective policy for Wasm execution; overridden,
non-Wasm, and build-only paths do not validate its YAML.
For `moon run -` and `moon run -e`, relative filesystem roots resolve from the
directory where Moon was invoked rather than the temporary source directory.

A native child receives the host user's ambient filesystem, network, and process
access. The `fs` and `net` objects do not sandbox child processes. PID-based
process operations are restricted to children spawned by the current moonrun
instance while policy mode is active.

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
