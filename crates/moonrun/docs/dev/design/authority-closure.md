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
reauthorizing the original pathname. Async Jobs retain their parent directory
resources and authorize derived operations using the path recovered from that
handle. Failed recovery never falls back to a saved opening path; missing-directory
errors deny admission, while other lookup errors keep their native mapping.
On macOS, the recovered pathname's device and inode must match the directory
handle before that path is used. `F_GETPATH` can retain a removed directory's name
even with a nonzero link count; a replacement at that name supplies no authority
for the original handle.
WASI directory descriptors also own directory handles and authorize derived
operations against their current locations.
Inherited and explicitly bound stdio are host-provided resource grants. Operations
on those handles, including metadata and locking, do not require pathname policy
authorization. Handle validity, native access modes and operation-specific handle
restrictions still apply; the grant does not authorize reopening a path or access
to children of a directory bound as stdio. Redirection through an authorized
guest spawn also grants the child stdio authority, independently of the source
resource's pathname policy. Attenuation of delegated stdio remains future work.
Other resources retain the rules below.

All filesystem authorization requests go through `HostFs::authorize` with an
`FsOperation`. Operations carry their targets: pathname lookup uses Runtime cwd
or an acquired-directory base, while handle operations use an opaque `FileGrant`.
Metadata and kind queries accept either target; entry changes require a pathname
and locks require an acquired file. A directory's handle grant cannot authorize
lookups beneath it. Directory-relative requests supply either an existing base
path or a live directory handle; the authorizer queries handle paths only when a
policy is present. There is no absent-base target state.

The authorizer interprets operation options, selects object or entry resolution,
and applies resource exemptions before matching resolved read/write requirements.
`FileGrant` distinguishes stdio, acquired access rights, and untracked resources.
Acquired grants retain the target's policy access at acquisition. Later Resource
operations check those rights without re-resolving the opening pathname; native
access modes and ABI descriptor rights remain additional constraints. New path
lookups, including children of an acquired directory, require path authorization.

`OpenAsync` retains scalar access/create modes and a positioning-only append flag.
`OpenWithOptions` retains the std::OpenOptions contract, where append implies
write and invalid creation/truncation combinations are rejected. Neither moves
validation ahead of policy. Async opening's optional extra metadata read is
checked after its open admission, using the same acquired-directory base.

Composite writes and recursive creation retain their existing target checks.
This internal distinction does not change literal roots, `"*"`, glob rules,
policy-group ordering, or rule transfer.

Filesystem glob exclusions override matching grants within the same read or write
list, independent of rule order. A matching positive rule is still required.
Exclusions constrain each authorized path; recursive removal and directory rename
currently authorize directory endpoints without visiting descendants. They do
not yet guarantee protection of excluded descendants from ancestor operations.

### Directory grants (initial scope)

An opt-in `dirs` map grants recursive access to existing directories, for example
`permission(dirs: { "work": ["write"], "work/private": ["read"] })` in `.mbtx`.
Write includes read; an empty permission list denies access. Roots are resolved
relative to the policy source at loading. The closest resolved directory wins
within a source; independent sources intersect. A source cannot combine `fs`
and `dirs`. Legacy `fs` configuration remains supported.

New acquisitions follow OS resolution, including symlinks and junctions, and
check the resulting target even when reached from a parent handle. Hard links
use the access path's authority; aliases are not detected or reconciled. A
writable alias can change data also reachable through a read-only directory.
Existing file grants still govern data and metadata together; OS modes and ABI
rights remain additional limits. Host-provided grants and built-in system reads
retain their existing exceptions.

This initial scope permits existing-object access and truncation. It rejects
creation, deletion, rename, composite writes that may create, writable SQLite
opens, and watching until their admission rules cover descendant restrictions.
Named stream/resource fork paths are also temporarily denied (TODO): authorize
them with the owning file's read/write rights while preserving inherited legacy
`fs` path restrictions.

`FsOperation` keeps implementation support and read/write requirements separate;
`HostFs` checks support before applying target rules or acquired-file grants.

From policy loading through its use, the caller must prevent uncoordinated
external changes to directory placement or lookup aliases, including relevant
ancestors, symlinks, junctions, mounts and hard links. Otherwise related new-path
access has no isolation guarantee; detection is not promised. Project membership
does not establish coordination across Runs. Existing grants and memory/resource
safety remain valid. Content changes, new ordinary files in stable directories,
and incidental effects such as atime are permitted.
Alternate mount views exposing the same subtree through different directory
roots are outside this initial model; it does not track global object identity.

### Operation ownership

Host Filesystem keeps operation interpretation, authorization, and effects
together. Jobs and synchronous operations share its domain rules. The async
runtime owns scheduling and outstanding request lifetimes; filesystem operations
may borrow request-owned resources without taking over their lifecycle. Raw
Operations supply OS mechanisms, and ABI adapters own guest representation.
This separation does not make pathname authorization and native lookup atomic.

Operation inputs determine both authorization intent and execution; callers
cannot supply a separate permission description that disagrees with the effects.
Policy denial remains distinguishable from I/O failure.

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

WASI `path_unlink_file` and `path_remove_directory` use the shared `unlinkat`
mechanism with separate file-only and directory-only operations. They do not
use async remove's automatic empty-directory fallback. On Windows the native
open enforces the requested entry kind before applying deletion disposition;
WASI file deletion retains std's read-only attribute handling. WASI and Async
opening share the `openat` mechanism. WASI keeps descriptor rights and preopen containment in its adapter;
HostFs authorizes the full open intent before execution. WASI directory and
preopen handles need traversal/metadata access, not enumeration access. Failure
to acquire the preopen fails run setup on every engine, even for modules that do
not import WASI; a constructed WASI context always owns a valid preopen. Async's
directory-listing opens additionally request listing access and asynchronous
Windows I/O. Synchronous opening does not enter worker cancellation.

Directory suffixes such as `link/` and `link/.` cause traversal, including for
operations that otherwise act on the final link entry. Entry authorization and
preopen containment must check that traversal. Restricted path resolution only
uses an existing ancestor for missing entries; it rejects unresolved existing
links, including dangling targets and loops. Ordinary missing files and
directories can be created when authorized. Removing or renaming a link checks
the entry rather than its target.

Relative opening and removal use the acquired directory handle, so renaming or
replacing its old path does not redirect those operations. Preopen containment and policy
still use pathname checks, and other WASI operations (including enumeration)
still use the directory's current path. These checks do not establish stable
identity against concurrent path replacement. Filesystem authority coverage
therefore does not imply protection against pathname and symlink races.

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
| `policy/source.rs` | Read `PolicyConfig`, resolve filesystem roots and glob bases, and separate authorization rules from Env provisioning |
| `Policy` | Share compiled rule groups, intersect complete policies, and supply domain policies without file I/O or parsing |
| Runtime Env provisioning | Construct Env from the selected source environment and provisioning rules |
| `policy/transfer.rs` | Encode the policy in an internal, versioned format; validate received rules and rebuild deterministic matchers without filesystem lookup |

`Rule` names an individual authorization rule, such as a `ProcessRule` matching
a program and argument prefix. `FsRules`, `NetRules`, and `ProcessRules` hold
one policy source's compiled rules. `Policy` retains those groups and supplies
`FsPolicy`, `NetPolicy`, and `ProcessPolicy`, which require every group to permit
the operation. Runtime passes these domain policies directly to their owners;
Host Process also retains the shared `Policy` for child inheritance.

`None` means no policy. A supplied document always creates a complete rule group,
including deny rules for omitted domains. There is no partially consumed policy
container or conversion back to user configuration during inheritance.

Virtual children receive typed restrictions. Their environment, cwd, and stdio
are separate construction inputs derived by Host Process. Policy inheritance
does not replay the parent's Env provisioning. An external child receives values
through its OS environment; Runtime construction owns that snapshot separately
from restriction decoding. The hidden serialized-input builder accepts only the
internal transfer format. It does not accept user policy documents.

User configuration is an input language, not an inheritance format. Transfer
records distinguish wildcard, IP, and DNS-name rules explicitly, preserve native
path bytes or units, and contain no Env provisioning. Domain adapters validate
received semantic values without reclassifying them from strings. Deterministic
caches such as Windows command-line prefixes may be rebuilt from the transferred
command and arguments. Filesystem globs carry a resolved base and a relative
pattern; exclusions use a distinct `exclude_glob` transfer variant, which older
readers reject. Decoding compiles the matcher without re-resolving the base or
interpreting a leading `!` again. These
caches do not require source files or ambient inputs.
Unknown versions, fields, and invalid rules fail decoding. The experimental
internal protocol evolves independently of the user configuration schema;
processes exchanging restrictions must support the same transfer version.

Resolved rules retain no policy-file reference. Changing the source document or
replacing a resolved root with a symlink must not redirect a descendant's grant.
Transfer decoding requires absolute roots (or the wildcard) and performs no
filesystem lookup. This preserves pathname rules, not stable directory identity
or protection against races during authorized operations.

Construction tests cover typed child state, transfer round trips, root replacement,
and source-file independence. Source selection and additional restriction
composition have a separate contract below.

## Policy composition

A child request must satisfy both the inherited policy and the selected child
policy. Unioning path roots, DNS patterns, or process argument lists does not
implement intersection. Trusted host grants are separate from guest derivation.

`Policy::intersect` joins compiled rule groups without file I/O, parsing, or Env
provisioning. Domain policies retain shared references to each group and require
every group to allow the operation. Positive alternatives within a group remain
alternatives, with filesystem exclusions taking precedence; groups are never
flattened into one allowlist.

| Input | Composition |
| --- | --- |
| Filesystem read/write rules | Every group must allow the same interpreted path and complete operation intent |
| DNS, connect, and bind rules | Every group must allow the corresponding network operation; DNS-derived grants only satisfy their originating group |
| Program and argument-prefix rules | Every group must allow the spawn request; admission does not confine the spawned native program |
| Env provisioning and cwd | Construction inputs, not authorization rules to intersect |
| Files, sockets, stdio, and directory handles | Resource grants with separate delegation contracts |

DNS-derived grants remain local to one Runtime and are not inherited with the
policy. Composition does not combine live resources or replay Env provisioning.

No child policy adds no rules. A supplied policy file is a complete policy:
omitted domains and empty allowlists deny that domain. Thus `{}` denies all
controlled domains, and a file containing only `net` denies filesystem and
process operations. This meaning is identical with and without an inherited
policy; partial additions are not supported. An explicit policy file selects
the child's policy instead of an
embedded declaration; neither can replace the inherited ceiling. Invalid or
missing selected child policies fail execution instead of being ignored.

Transfer version 3 carries a nonempty `rule_groups` sequence,
including when only one group is present. Older versions are rejected. Ordinary
string rules retain their user-facing semantics. Decoding preserves group
boundaries so conjunction and DNS provenance survive repeated inheritance.
Unknown versions and empty collections fail closed.

Environment provisioning is consumed during construction. Child provisioning
reads the delegated source Env, and inherited transfers contain no captured
values or provisioning to replay. The resulting Env travels through the normal
child-environment mechanism.

Acceptance cases cover repeated derivation and transfer, absent and empty child
policies, overlapping path roots, command prefixes, DNS-derived grants, and
attempts to widen inherited restrictions. Unit tests in `policy/composition_tests.rs`
and the domain tests cover these rules; `tests/policy_composition.rs`,
`tests/moon_handoff.rs`, and the Moon/moonx tests cover construction and process
transport. Composition of domain policies does not implement resource
attenuation or native confinement.

## Resource delegation

Delegation must identify the resource's owner, kind, lifetime, permitted use,
and attenuation on transfer. Copying a Handle number is not delegation.
Acquired files, sockets, stdio, and directories must retain their granted object
identity without reopening their names or silently widening rights.

Stdio redirection delegates the opened object and its OS access mode. A spawn
Job retains acquired Resources after the parent closes its Handles; native
spawn duplicates the descriptors, and virtual spawn gives the child owned
files. Releasing a failed or discarded Job releases its references. Child path
rules restrict new opens, not I/O on these grants. Naming a stream stdin or stdout
does not attenuate an OS handle: async I/O retains its native access mode,
while WASI additionally applies its descriptor rights. Runtime-owned standard
streams cannot be closed through guest Handle APIs.

Acceptance cases must reject foreign or stale Handles, preserve in-flight
resource ownership through guest close, and show that transfer cannot increase
rights beyond the delegated grant. General cross-Runtime delegation requires
explicit rebinding; a shared handle number is insufficient.

## Other host effects

SQLite's synchronous and Job opens share admission checks: reading requires
access to the database and its parent directory; writable opens require write
access to both. A directory grant does not authorize a database symlink target
outside that grant. These checks precede SQLite's open and are not VFS mediation.

The [built-in system access inventory](../system-access.md) records timezone read grants and host trust-provider activity.

Each exposed host effect needs an explicit contract: an enforced rule,
trusted host work, or unrestricted exposure. Filesystem policy alone does not
mediate SQLite auxiliary files, TLS trust discovery, clocks, randomness, or
library initialization. Tests for those surfaces must exercise indirect effects
as well as the immediate import.

On Windows, a guest can query the configured system timezone under an empty
policy. Each query uses a timezone handle, so the host selects the registry key.
See [Windows timezone imports](../windows-timezone-abi.md) for the scope of
these reads.

Helper and native execution need explicit grants coordinated with execution
ownership. Permission to invoke a program does not confine the program's
filesystem, network, or process access. Guest Wasm rules do not sandbox build
helpers or arbitrary native syscalls. Any stronger claim requires separate
enforcement and acceptance tests.
