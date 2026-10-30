# Authority closure workstream

Status: the filesystem increment is implemented; subsequent increments remain
planned. Validation evidence is recorded with each change.

This workstream implements the authority requirements in
[ADR 0012](../../adr/0012-preserve-embeddability-and-runtime-owned-host-semantics.md)
and the [runtime architecture guide](runtime-architecture.md#overall-authority-and-ffi-mapping).
It does not replace those decisions or introduce another policy schema keyed
by FFI names. Command selection, preparation-helper cancellation, and managed
native/Node execution are separate workstreams.

## Ownership and coordination

Host-global lifetime and guest authorization answer different questions. The
first determines who initializes, changes, and releases a host facility without
interfering with another embedded Engine. The second determines which effects
one Runtime may request. A correct authorization check does not repair a
process-global lifetime bug, and a lifetime guard does not grant guest access.

| Work | Recommended owner | Contract with this workstream |
| --- | --- | --- |
| Moon argument parsing and execution selection | Command-selection work | Classification must not grant authority or discard inherited restrictions |
| Helper groups and Windows Jobs | Preparation/native-execution work | Cancellation membership is separate from permission; a Job alone does not enforce Moonrun filesystem/network policy |
| Native and Node execution | Native-execution work | Obtain explicit authority for external execution before launch; do not imply that Wasm policy confines native syscalls |
| Unix signal dispositions and worker cancellation signals | Independent embedding work | Coordinate host-global ownership across Engines and with the embedding caller |
| Winsock initialization and cleanup | Independent embedding work | Guest calls must not release another Runtime's or the embedding caller's initialization; coordinate changes to exposed imports |
| Diagnostics | Independent embedding work | Route through an explicit host-owned destination; authorization errors remain domain results |
| Domain rules, WASI, resource grants, child restrictions | Authority work | Enforce the delegated ceiling through every exposed route |

Fix a concrete cross-Runtime authority violation when it blocks an authority
increment, but keep unrelated embedding cleanup out of that PR. In particular,
do not move process-group or Job ownership into the policy module.

## Source audit and remaining gaps

- The filesystem increment closes the former gap between Runtime filesystem
  rules and WASI's independent preopen/descriptor checks. `WasiContext` retains
  the Runtime's Host Filesystem and invokes it for authorized filesystem
  effects. Descriptor rights and path containment remain additional checks.
- Runtime construction selects inherited policy ahead of a policy file.
  `Engine::execute_moon_in_scope` also selects delivered policy only when neither
  configured nor inherited policy exists. This preserves parent restrictions
  in the existing path but does not apply an additional child's restrictions.
  General restriction composition remains unimplemented.
- Policy inheritance transports canonical domain configuration and inherits
  the child's supplied environment. Environment provisioning is state
  construction, not an authorization rule to merge mechanically with file or
  network permissions.
- The Windows `runtime/init_WSA` and `runtime/cleanup_WSA` imports directly call
  the raw functions. Worker initialization installs process-wide Unix signal
  dispositions, and the Windows ambient process implementation initializes a
  global Job containing the host process. These are concrete embedding audit
  targets, not evidence that all global facilities belong in authority policy.
- Policy denial currently writes directly to host stderr. Error reporting and
  diagnostic ownership should remain distinct from the allow/deny decision.

## First increment: filesystem authority across FFI and WASI

Done condition: when a Runtime filesystem policy denies a filesystem effect,
WASI cannot perform that effect through its cwd preopen or a directory derived
from it. No-policy controls preserve native operation semantics; a capability
escape is a correctness bug even when no Runtime policy is configured.

The implementation uses the existing `HostFs` semantic owner for filesystem authorization. WASI
continues to own descriptor lookup, descriptor rights, dirfd-relative path
interpretation, and WASI errno mapping. Once those semantics identify the
operation and target, the operation must also satisfy filesystem policy before
its protected effect. Do not apply Runtime cwd a second time to a path already
resolved from a WASI directory. Avoid a generic per-import permission dispatcher
or a new global policy check around every raw operation.

Cover the implemented filesystem surface, not only `path_open`: metadata and
directory enumeration, link inspection, creation/truncation, deletion, and both
targets of rename. Opening a file must account for granted later descriptor
operations as well as immediate creation/truncation effects. Subsequent I/O on
that acquired file uses its retained grant and WASI rights, rather than
reauthorizing its old pathname. Directory descriptors currently retain paths,
so descendant path operations still require authorization. Explicit stdio
bindings are separately delegated resources, not grants inferred from cwd.

Acceptance tests (`wasi/authority_tests.rs` and `tests/filesystem_authority.rs`):

- A denied read/write fails through synchronous MoonBit imports, async imports,
  and WASI; each ABI retains its own error representation.
- A read-only grant permits reads but prevents creation, truncation, and writes;
  rejected operations leave the filesystem unchanged.
- Directory-derived operations and both sides of rename obey the same ceiling.
- WASI descriptor rights and preopen containment remain required even when
  filesystem policy allows the target.
- No-policy controls preserve existing results across supported backends, with
  tests organized by semantic operation and shared adapter coverage.

`filesystem/path_ops.rs` owns synchronous authorization plus execution, using
`std::fs` and `filesystem/native.rs` for native mechanisms. Core directory enumeration and
file removal reuse the same operations. `FileOpenOptions` supplies both the
native options and the authorization intent; WASI does not pass a separate
permission description that could disagree with execution. Domain errors
preserve policy denial separately from native I/O errors for ABI translation.

The three layers remain ABI adaptation, Host Filesystem interpretation and
authorization, and native mechanisms. WASI does not invoke async Jobs to reuse
an OS operation. Jobs retain scheduling, cancellation, and result ownership.
Both paths reuse the raw rename implementation, with explicit replacement and
cross-device options. WASI selects `CrossDevice::Reject` and reports `XDEV`;
async retains `AllowCopy`, which permits its existing Windows fallback but does
not promise copying on Unix. Ordinary replacement uses the standard library
on Unix, and also on Windows when copying is rejected; this preserves existing
syscall availability and Windows long-path handling. This parameter controls the mechanism, not guest
authority. A directory handle alone would not guarantee a single filesystem.

Other differences remain at the layer that owns them: WASI `path_unlink_file`
must not inherit async remove's empty-directory behavior, while synchronous
open and I/O can use standard-library primitives without worker cancellation.
Directory suffixes such as `link/` or `link/.` require traversal checks, even
for operations normally acting on a link entry. Both the shared entry policy
and WASI preopen boundary check the traversed target. A dangling link with such
a suffix is rejected conservatively; an ordinary missing directory name can
still be created. Tests cover policy denial and no-policy preopen containment.

This increment closes an alternate access route. It does not establish stable
file identity against concurrent path replacement: the existing pathname and
symlink race limitations remain explicit until directory-handle work lands.

## Subsequent increments

1. Compose inherited domain restrictions with additional child restrictions.
   A request must satisfy every applicable restriction; an absent additional
   restriction adds nothing, while an explicitly empty allowlist denies that
   domain. Do not approximate intersections of path roots, DNS patterns, or
   process argument rules by unioning their configuration lists. Keep trusted
   host grants distinct from guest derivation and preserve restrictions through
   repeated child derivation and serialization.
2. Make resource delegation explicit: owner, kind, lifetime, permitted use, and
   any attenuation on transfer. A copied Handle number is not delegation.
   Cover stdio and acquired files/sockets without reopening their original
   names or silently widening rights.
3. Audit remaining host effects and native-library side effects, including
   SQLite auxiliary files, TLS trust discovery, clocks/randomness, and host
   initialization imports. Record unrestricted exposure, trusted host work, or
   an enforced rule explicitly; do not infer coverage from filesystem policy.
4. Integrate helper/native execution grants with the separate execution
   workstreams. Distinguish permission to invoke a native program from
   confinement of what that program can do. Until enforceable, do not describe
   native/helper execution as constrained by guest Wasm domain rules.

Each increment needs its own completion claim and tests. Passing the first
increment does not mean total authority, native confinement, or embedding
isolation is complete.
