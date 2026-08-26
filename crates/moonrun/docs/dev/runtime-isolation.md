# Runtime Isolation Contract

Moonrun is moving toward running multiple mutually untrusted Wasm guests in one
host process. This document defines how that work can proceed without changing
the behavior of existing users or claiming isolation that Moonrun does not yet
provide.

The current implementation is a compatibility baseline, not a complete
sandbox. A Runtime owns some state, but several dependencies deliberately remain
Ambient and observable process-wide.

## Scope

The isolation target treats guest Wasm as untrusted and Moonrun, its native
dependencies, and the operating system as the trusted computing base. It does
not protect against arbitrary native code loaded into the moonrun process or a
bug in a Host-domain implementation. A sandbox claim covers only the
guest-reachable effects named in its platform support matrix.

Spawned native processes and hidden I/O performed by native libraries require
their own audited adapters. They do not become isolated merely because the
guest call that reached them crossed a Runtime interface.

## Correctness Comes First

Runtime behavior and authorization are independent concerns:

```text
Runtime behavior:  Ambient | future isolated adapters
Authorization:     unrestricted | Moonrun Policy
```

The following rules are non-negotiable:

1. Running without a policy preserves the historical observable behavior of
   moonrun. A structural refactor must not snapshot an Ambient value, change
   when it is observed, normalize a native value, substitute a different
   platform default, or translate a native error differently.
2. When Moonrun Policy authorizes an operation, that operation should retain the
   unrestricted platform semantics unless an independently selected Runtime
   adapter explicitly defines different behavior. Policy decides whether an
   operation may proceed; it does not select cwd, executable lookup, DNS, or
   other operating-system semantics.
3. Policy denial must occur before the denied OS effect. Authorization must not
   rewrite the guest request and then execute the rewritten request as though it
   were equivalent.
4. The `env` section of a policy file contributes Runtime environment
   configuration as well as permission selection. Code should treat the
   realized Env as Runtime state rather than using the presence of a policy as
   a general signal to change unrelated behavior.
5. An isolation adapter may deliberately provide narrower behavior than the
   native process environment, but that behavior must be explicit, documented,
   opt-in, and fail closed when unsupported. It must not silently replace the
   Ambient default.

An allow-all policy with equivalent Runtime configuration should therefore be
observationally equivalent to unrestricted execution for operations covered by
that policy. This is a compatibility property, not a claim that Policy is an OS
syscall sandbox.

## Authority and Ownership

The Wasm guest cannot issue native syscalls directly. Moonrun's primary
in-process isolation seam is therefore the set of host operations exposed to
the guest.

- Engine owns facilities that are inherently process-shared, such as V8 setup.
- Runtime is the composition root for one virtual environment.
- Host Filesystem, Host Network, Host Process, and other Host-domain modules own
  the guest-visible operation from authorization through the OS effect and
  result interpretation.
- For Host-domain surfaces, Wasm backend adapters decode ABI values and borrow
  Guest Memory; they do not independently choose authority or perform an
  equivalent OS operation. WASI remains a separately configured and audited
  capability surface.
- Resources that represent Runtime authority should be owned handles, tokens,
  or immutable configuration. Per-Runtime isolation must not be implemented by
  changing process-global state and restoring it later.

Where correctness depends on a stable filesystem identity, future adapters
should prefer handle-relative operating-system operations. Checking one path
and later executing an ordinary path-based operation leaves rename, symlink,
and cwd races between authorization and use. Moonrun should delegate native
path and executable-search semantics to platform facilities where possible,
not grow a cross-platform emulation layer.

An Anchored working-directory mode must not be added merely by resolving guest
paths into `PathBuf` values. It requires each affected Host domain to combine
the anchor's authority with the actual operation. Until that implementation is
available for the promised surfaces, the only working-directory behavior is
Ambient.

## Ambient Effect Inventory

This table records the important process or OS dependencies known today. It is
an inventory of correctness and ownership work, not a promise that every row is
already isolated.

| Effect | Current status | Current correctness contract | Isolation direction |
| --- | --- | --- | --- |
| Working directory | Ambient | Observe or inherit the process cwd at the historical execution points; do not snapshot or change it | Add an opt-in handle-backed adapter only after affected Host domains can execute relative to its authority |
| Environment | Mixed | Unrestricted mode retains process-environment behavior and requires serialization around concurrent mutation; policy mode realizes a Runtime-owned Env used by guest environment APIs, WASI environment calls, temporary-directory selection, and child inheritance | Keep derived behavior tied to the declared Env while auditing libraries that read the process environment directly |
| Filesystem | Mixed | Moonrun-owned imports use Host Filesystem; relative operations retain native path behavior; WASI descriptors remain a separate capability surface | Move authorization and execution onto stable handle-relative operations one platform at a time |
| Temporary directory | Mixed | Preserve the native source in unrestricted mode; a configured Runtime Env determines the guest-facing result in policy mode | Represent an isolated temporary directory as explicit Runtime authority instead of reproducing platform fallback rules |
| Process spawning | Mixed | Policy authorizes the logical request; native executable lookup remains platform behavior; the child receives the realized Runtime Env and currently retains ambient host authority | Add a separate OS-enforced child sandbox adapter; do not infer executable-search semantics merely from policy presence |
| Standard I/O | Ambient/Mixed | Existing imports and default child spawning retain inherited process stdio; some async resources already hold native handles | Give Runtime explicit handle-capable I/O authority before promising independent redirection, polling, or child inheritance |
| Network and DNS | Mixed | Host Network authorizes moonrun-owned socket operations; name resolution still observes the host resolver and its caches | Keep socket authority in Host Network; add a resolver adapter only when tenant isolation or deterministic replicas require one |
| Signals | Process-global | Preserve existing signal compatibility behavior | Place unavoidable process-wide disposition and delivery behind an Engine-owned broker; do not save and restore it per Runtime |
| TLS trust roots | Ambient | Existing native TLS setup may observe process environment and system trust configuration | Supply trust roots explicitly to a Runtime-owned TLS path before claiming environment isolation for TLS |
| SQLite VFS and temporary files | Mixed | SQLite Host owns guest-visible database state, while the native VFS may perform ambient filesystem and temporary-file operations | Configure a per-connection or Runtime-owned VFS path; avoid per-Runtime mutation of SQLite globals |
| Clock, randomness, PID, and OS metadata | Ambient | Preserve native observations | Treat deterministic replicas as a separate requirement from tenant isolation and add adapters only when required |
| Credentials, umask, resource limits, and other process attributes | Ambient | Preserve host process behavior | Audit before any sandbox claim; prefer kernel-enforced authority or explicit unsupported status over process-global mutation |

Any new direct read or mutation of process-global state must either be routed
through the owning module or added to this inventory with its compatibility
contract. Third-party libraries that perform hidden I/O are part of this audit
even when the guest-facing call already crosses a Host-domain module.

## Incremental Delivery

Sandboxing proceeds in stages. Each stage must be independently reviewable and
must leave the default behavior usable.

1. **Freeze the baseline.** Add black-box compatibility tests for Ambient,
   unrestricted execution and for policy-authorized operations.
2. **Concentrate effects.** Route each guest-visible OS effect through its
   Runtime-owned Host-domain module without changing the underlying operation.
3. **Establish ownership.** Replace borrowed process state with Runtime-owned
   handles or data where doing so is behavior-preserving. Add concurrent Runtime
   and teardown tests. Deliberately Ambient dependencies stay explicit.
4. **Add one isolated adapter.** Start with filesystem authority because cwd,
   path replacement, and symlink correctness meet there. Keep Ambient as the
   default and avoid exposing a new mode until the adapter can honor its full
   interface.
5. **Apply platform hardening.** Use the strongest appropriate native
   facilities on each platform. Platforms need not share an implementation, and
   a missing reliable primitive is reported as unsupported rather than replaced
   with a weaker approximation.
6. **Expand the claim.** Add other Host domains one at a time and publish a
   support matrix. Moonrun should call a configuration a sandbox only when every
   guest-reachable effect included in that claim has been audited.

Structural movement, ownership changes, and new observable semantics should be
separate changes. This keeps compatibility regressions attributable and lets an
unfinished isolation adapter remain opt-in without weakening Ambient behavior.

## Verification Gates

Every isolated Host domain needs both compatibility and isolation verification:

- unrestricted Ambient behavior before and after the refactor;
- an equivalent allowed operation with Policy enabled;
- denied operations producing no partial OS effect;
- native errors and platform path representations passing through unchanged;
- concurrent Runtimes not sharing owned state or handles accidentally;
- dropping one Runtime not restoring or invalidating another Runtime's state;
- adversarial rename, symlink, deletion, and handle-lifetime cases where they
  apply;
- an explicit result for every supported platform: enforced, Ambient by
  contract, or unsupported.

Isolation, deterministic replication, and sandboxing are related but distinct.
Isolation prevents one Runtime from mutating another's authority; deterministic
replication controls observations such as time and randomness; sandboxing
restricts access to host resources. A change should state which property it
improves instead of assuming progress on one proves the others.
