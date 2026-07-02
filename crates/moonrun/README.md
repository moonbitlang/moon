![check](https://github.com/moonbitlang/moonrun/actions/workflows/ci.yml/badge.svg)

# moonrun

Moonrun is the WebAssembly runtime for MoonBit, utilizing V8 at its core to offer an efficient and flexible environment for executing WASM.

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

## Experimental Policy

By default, running `moonrun` without `--policy` preserves existing behavior.

Supplying `--policy <path>` enables an experimental policy system and switches
supported moonrun-owned host surfaces into sandbox mode. The policy is
deny-by-default: omitted or empty `[fs]`, `[net]`, and `[env]` sections deny
that surface. Add entries only for the access the program should have. The
policy covers `moonbitlang/async` and moonrun's own `__moonbit_*_unstable` FFI
surfaces. It does not apply to WASI
(`wasi_snapshot_preview1` / `__moonbit_wasi_unstable`).

An empty policy file denies all policy-covered filesystem, network, and
environment access:

```toml
# deny-all.toml
```

To allow everything while still passing a policy file, use explicit wildcards:

```toml
[env]
from_host = ["*"]

[fs]
read = ["*"]
write = ["*"]

[net]
dns = ["*"]
connect = ["*:*"]
bind = ["*:*"]
```

The simplest way to preserve legacy allow-all behavior is still to run without
`--policy`. The allow-all policy is useful when another tool requires a policy
file during migration or debugging.

The filesystem policy restricts native host paths. It does not create a virtual
guest filesystem, mount table, or portable `/` namespace. Relative filesystem
roots are resolved relative to the policy file. Runtime relative paths are
resolved using the process current directory. Paths use the host platform's path
syntax; Windows policies may use normal Windows paths such as `C:\work` or
`C:/work`. The filesystem wildcard `"*"` allows every host path on every
platform. List a root in both `read` and `write` to allow read-write filesystem
access.

The environment policy constructs the guest environment. Use `from_host` to copy
selected host variables if present, `required_from_host` to require selected
host variables, and `[env.set]` for literal values. `[env.set]` overrides values
copied from the host. Do not put secrets directly in the policy file; pass them
by name through `from_host` or `required_from_host`.

```toml
[env]
from_host = ["PATH", "SSL_CERT_FILE", "SSL_CERT_DIR"]
required_from_host = ["DEEPSEEK_API_KEY"]

[env.set]
APP_ENV = "prod"
API_BASE = "https://api.deepseek.com"

[fs]
read = ["allowed"]
write = ["scratch"]

[net]
connect = [
  "api.deepseek.com:443",
  "hacker-news.firebaseio.com:443",
  "127.0.0.1:443",
  "[::1]:*",
]
bind = ["127.0.0.1:*"]
```

To allow outbound access only to DeepSeek and Hacker News:

```toml
[net]
connect = [
  "api.deepseek.com:443",
  "hacker-news.firebaseio.com:443",
]
```

Hostname entries in `connect` allow DNS lookup for that host and allow
connections to the IP addresses returned by that lookup on the configured port.
Use `dns` only when a program needs standalone DNS lookup permission without
also granting outbound connects.

# Contribution

To contribute, please read the contribution guidelines at [docs/dev](./docs/dev/README.md).
