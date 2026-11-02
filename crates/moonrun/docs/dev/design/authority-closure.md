# Authority closure

This design specializes the authority requirements in
[ADR 0012](../../adr/0012-preserve-embeddability-and-runtime-owned-host-semantics.md)
and the [runtime architecture guide](runtime-architecture.md#overall-authority-and-ffi-mapping).
It defines ownership, enforcement rules, and acceptance criteria. Implementation
status and work dependencies belong in the
[architecture tracker](../architecture-tracking.md#authority-work).

## Ownership

Host-global lifetime determines who initializes, changes, and releases a host
facility without interfering with another Engine or its embedding caller.
Guest authorization determines which effects a Runtime may request. A lifetime
guard does not grant access, and an authorization check does not provide
process-global isolation.

| Concern | Semantic owner | Authority contract |
| --- | --- | --- |
| Moon argument interpretation | Moon's parser and Host Process execution selection | Classification must not grant authority or discard inherited restrictions |
| Helper groups and Windows Jobs | Preparation and native-execution lifecycle | Cancellation membership is separate from permission; a Job alone does not enforce filesystem or network policy |
| Native and Node execution | Host Process admission and Engine execution | Permission to launch does not imply confinement of native syscalls |
| Signal dispositions and Winsock lifetime | Host-global coordination | One Runtime must not release or reconfigure another Runtime's or the embedding caller's facilities |
| Diagnostics | Embedding-owned destination | Reporting a denial is separate from deciding it |
| Domain rules and resource grants | The owning Host Domain | Every exposed route must respect the delegated authority ceiling |

Process-group and Job ownership belongs with execution, not with policy.
Environment provisioning constructs Runtime state; it is not a restriction to
merge mechanically with filesystem or network rules.

## Filesystem authority

A Runtime filesystem policy applies across unstable, async, and supported WASI
filesystem operations. Switching ABI must not bypass a denied effect.

Host Filesystem owns path interpretation, filesystem authorization, and effects.
WASI owns descriptor lookup, rights, dirfd-relative path interpretation, preopen
containment, and errno mapping. Both checks must allow the operation. A path
resolved from a WASI directory must not receive Runtime cwd a second time.

Authorization covers metadata, directory enumeration, link inspection,
creation/truncation, deletion, and both rename endpoints as well as opening.
Open options determine both the immediate effects and the acquired file grant.
I/O on that file uses its grant and descriptor rights without reopening or
reauthorizing the original pathname. Directory descriptors retain paths, so
operations derived from them require path authorization. Explicit stdio bindings
are separate resource grants.

### Implementation responsibilities

| Module | Responsibility |
| --- | --- |
| `filesystem/host.rs` | HostFs state, path interpretation, authorization, synchronous effects, and domain `FsError` results |
| `filesystem/job/` | Filesystem Job payloads, execution, and result ownership |
| `std::fs` and `filesystem/os.rs` | OS mechanisms; the custom module accepts host paths and `RenameOptions`, returns `io::Result`, and has no Runtime, policy, or ABI dependency |
| `unstable_api/filesystem.rs` | Legacy error strings, status codes, and per-run getter state |
| WASI and async adapters | Their own rights, ABI values, and error representation |

`filesystem/mod.rs` declares modules and exports. `FileOpenOptions` supplies
both OS options and authorization intent; callers cannot pass a separate
permission description that disagrees with execution. `FsError` distinguishes
policy denial from I/O failure.

A lazy write payload is authorized before decoding and decoded before file
creation or truncation. Guest conversion remains in the adapter. WASI does not
submit an async Job merely to reuse an OS operation.

### Platform semantics

The shared rename operation has explicit replacement and cross-device options.
WASI selects `CrossDevice::Reject` and maps cross-device errors to `XDEV`.
Async selects `AllowCopy`, permitting its Windows fallback without promising
copying on Unix. Ordinary replacement uses the standard library on Unix, and
on Windows when copying is rejected, preserving syscall availability and
long-path handling. These options control mechanisms, not guest authority.
A directory handle does not imply that every descendant lies on one filesystem.

WASI `path_unlink_file` retains file-only removal rather than async remove's
empty-directory behavior. Synchronous open and I/O use standard-library
primitives without worker cancellation.

Directory suffixes such as `link/` and `link/.` cause traversal, including for
operations that otherwise act on the final link entry. Entry authorization and
preopen containment must check that traversal. Dangling links with directory
suffixes are rejected conservatively; ordinary missing directories can be
created when authorized.

Pathname checks do not establish stable identity against concurrent path
replacement. Filesystem authority coverage therefore does not imply protection
against pathname and symlink races.

### Filesystem acceptance criteria

[WASI authority tests](../../../src/wasi/authority_tests.rs) and
[cross-ABI tests](../../../tests/filesystem_authority.rs) exercise these rules:

- A denied read or write fails through unstable, async, and WASI, with each
  ABI retaining its error representation.
- A read-only grant permits reads and prevents creation, truncation, and writes;
  rejection leaves the filesystem unchanged.
- Directory-derived operations and both rename endpoints obey the policy.
- WASI rights and preopen containment remain required when policy allows access.
- Acquired-file grants and explicit stdio bindings retain their permitted use.
- No-policy controls preserve platform behavior. Tests distinguish policy denial
  from OS errors and cover directory suffixes and links.

## Runtime construction and inheritance

Construction separates four responsibilities:

| Owner | Input and output |
| --- | --- |
| `policy/source.rs` | Read a policy document, resolve source-relative filesystem roots, and separate restrictions from Env provisioning |
| `PolicyInheritance` | Retain validated, immutable domain rules with resolved roots; supply domain policies without file I/O or parsing |
| Runtime Env provisioning | Construct Env from the selected source environment and provisioning rules |
| Policy transfer adapter | Encode resolved rules for an external process and validate received rules without resolving their roots again |

Virtual children receive typed restrictions. Their environment, cwd, and stdio
are separate construction inputs derived by Host Process. Policy inheritance
does not replay the parent's Env provisioning. External process transfers use
the existing policy schema; its Env wildcard selects values passed through the
normal child environment. The hidden serialized-input builder is a transport
entry point, not the in-process inheritance interface.

Resolved rules retain no policy-file reference. Changing the source document or
replacing a resolved root with a symlink must not redirect a descendant's grant.
Transfer decoding requires absolute roots (or the wildcard) and performs no
filesystem lookup. This preserves pathname rules, not stable directory identity
or protection against races during authorized operations.

Construction tests cover typed child state, transfer round trips, root replacement,
and source-file independence. Source selection and additional restriction
composition have a separate contract below.

## Restriction composition

A child request must satisfy delegated parent restrictions and any additional
child restriction. An absent additional restriction adds nothing; an explicitly
empty allowlist denies that domain. Unioning path roots, DNS patterns, or process
argument lists does not implement intersection. Trusted host grants are a
separate operation from guest child derivation.

Acceptance cases must cover repeated child derivation and serialization, empty
and absent restrictions, and child attempts to widen parent grants. These are
requirements for composition, not guarantees supplied by policy precedence:
Runtime construction selects inherited policy ahead of a policy file, and
Engine selects delivered policy only when neither configured nor inherited
policy exists. That selection does not apply additional child restrictions.

## Resource delegation

Delegation must identify the resource's owner, kind, lifetime, permitted use,
and attenuation on transfer. Copying a Handle number is not delegation.
Acquired files, sockets, stdio, and directories must retain their granted object
identity without reopening their names or silently widening rights.

Acceptance cases must reject foreign or stale Handles, preserve in-flight
resource ownership through guest close, and show that transfer cannot increase
rights beyond the delegated grant. General cross-Runtime delegation requires
explicit rebinding; a shared handle number is insufficient.

## Other host effects

Each exposed host effect needs an explicit contract: an enforced rule,
trusted host work, or unrestricted exposure. Filesystem policy alone does not
mediate SQLite auxiliary files, TLS trust discovery, clocks, randomness, or
library initialization. Tests for those surfaces must exercise indirect effects
as well as the immediate import.

Helper and native execution need explicit grants coordinated with execution
ownership. Permission to invoke a program does not confine the program's
filesystem, network, or process access. Guest Wasm rules do not sandbox build
helpers or arbitrary native syscalls. Any stronger claim requires separate
enforcement and acceptance tests.
