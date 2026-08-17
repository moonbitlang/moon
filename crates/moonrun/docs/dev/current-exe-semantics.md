# Current executable path semantics

Status: design research, 2026-08-26.

This note distinguishes a path *name* from the identity of the executable
object. No major native API promises that its result still resolves to the
object whose code is running. Rust consequently documents
`std::env::current_exe` as platform-specific: symlink handling differs, a
rename may leave the load-time path in place, the operation may fail, and the
result is unsafe as a security identity
([Rust API contract](https://doc.rust-lang.org/std/env/fn.current_exe.html)).

The most useful conclusion for moonrun is:

> Capture the absolute, non-canonicalized path used to load a file-backed wasm
> module, once, when that module is loaded. Keep returning that snapshot even
> if the file or one of its ancestors is later renamed or removed. An in-memory
> module has no current executable path unless its API supplies one explicitly.

This deliberately chooses a stable guest contract rather than trying to
reproduce every host's mutation behavior.

In the platform sections below, **documented** means an API or manual-page
guarantee; **implementation-derived** means behavior shown by the cited OS or
runtime source but not promised as a stable API contract; and **inferred**
means the expected consequence of those mechanisms. Mutation details are not
silently promoted from the latter two categories into portable guarantees.

## Rust's actual platform calls

The public Rust contract is intentionally weaker than “the current name of the
same file.” Its current standard-library implementations use different OS
facilities
([Unix source](https://doc.rust-lang.org/stable/src/std/sys/paths/unix.rs.html),
[Windows source](https://github.com/rust-lang/rust/blob/main/library/std/src/sys/paths/windows.rs)):

| Target | Rust implementation |
| --- | --- |
| Windows | `GetModuleFileNameW(NULL, ...)` |
| Linux/Android | `readlink("/proc/self/exe")` |
| macOS and other Apple targets | `_NSGetExecutablePath` |
| FreeBSD/DragonFly | `sysctl(KERN_PROC_PATHNAME, -1)` |
| NetBSD | `sysctl(KERN_PROC_PATHNAME)`, then `/proc/curproc/exe` |
| OpenBSD | obtains `argv[0]`; canonicalizes it only when it starts with `.` or contains `/` |
| Solaris/illumos | `/proc/self/path/a.out`, falling back to `getexecname()` |
| AIX | resolves `argv[0]` using the current directory or current `PATH`, then canonicalizes |

“Unix semantics” therefore do not exist as one behavior.

## Windows

### `GetModuleFileNameW` (what Rust uses)

With a null module handle, `GetModuleFileNameW` returns the current process's
executable path. Microsoft describes it as fully qualified and says its string
uses the same format supplied when the module was loaded, including long/short
name spelling and a possible `\\?\` prefix
([Microsoft documentation](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-getmodulefilenamew)).

That is a loaded-module name, not a file-identity handle. The documented API
does not define a complete rename/delete mutation protocol. The safe contract
is therefore the load spelling, potentially stale:

| Circumstance | Result/constraint |
| --- | --- |
| Normal call | Fully qualified loaded-module path in the form recorded at load time. |
| Symlink, junction, short name, or `\\?\` input | The API preserves the recorded load format; it does not promise a canonical final path. |
| Executable renamed or moved on the same volume | The API does not promise to follow the rename. Treat the returned loaded-module name as the old, potentially stale path. |
| Ancestor directory renamed or moved | Same: loader metadata is not a live directory watch, so callers must allow the old path. |
| Executable deleted | Ordinary file-system semantics prevent deleting an executable while its image section is mapped. Microsoft explicitly notes that a mapped executable cannot be deleted even without an open file handle ([executable-image semantics](https://learn.microsoft.com/en-us/windows-hardware/drivers/ifs/executable-images)). |
| Ancestor directory removed | Ordinarily impossible while it still contains the mapped executable. If an unusual file system or disconnection makes the name disappear, the path API does not guarantee that its returned name exists. |
| Old path replaced with another file | A cached path can now resolve to the replacement. It is not proof that the replacement contains the executing image. |
| Current directory changed | No effect on the already fully qualified loaded-module name. |
| Cross-volume “move” | `MoveFileEx(..., MOVEFILE_COPY_ALLOWED)` is copy plus delete; Microsoft says it can report success with the source left intact when deletion fails ([`MoveFileExW`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)). A running image therefore normally remains at its source name. |

The rename rows are a consequence of the loaded-name model, not a normative
Microsoft promise that every Windows version and file system must behave
identically. The API's explicit guarantee stops short of post-load mutation
semantics. In this table, the normal/form-preservation behavior and mapped-file
deletion constraint are documented; the rename, ancestor-rename, replacement,
and unusual-disconnection outcomes are conservative inferences from the
loaded-name mechanism.

### `QueryFullProcessImageNameW` (not what Rust uses)

`QueryFullProcessImageNameW` queries a process handle and can request either a
Win32 path or a native-system path. Its documentation promises the full image
name and defines access, buffer, and error behavior, but says nothing about
whether a later rename, move, or deletion must be reflected
([Microsoft documentation](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew)).
It is therefore not a sound basis for a more precise portable mutation
contract. Switching moonrun between these Windows calls would also not solve
the guest problem: either call would identify `moonrun.exe`, not the wasm
module.

## Linux

Linux is materially different. `/proc/<pid>/exe` refers to the executable file
held by the process. The kernel obtains that file's resolved `f_path` and
formats it with `d_path`
([`proc_exe_link` and readlink implementation](https://github.com/torvalds/linux/blob/master/fs/proc/base.c),
[`d_path` implementation](https://github.com/torvalds/linux/blob/master/fs/d_path.c)).
The formatter walks the current dentry/parent chain under the rename lock, and
marks an unlinked dentry with ` (deleted)`.

| Circumstance | `/proc/self/exe` / Rust result |
| --- | --- |
| Normal call | Absolute path derived from the executable file's current resolved dentry in the caller's root/mount view. |
| Invoked through a symbolic link | Target path, because the stored executable `f_path` is the resolved file, not the symlink argument. |
| Invoked through one of several hard links | The dentry used for execution is retained. Mutating another hard link does not change it. |
| Executable renamed or moved on the same mounted file system | The next call normally reports the new path. |
| Ancestor directory renamed or moved | The next call normally reports the path through the ancestor's new name/location. |
| Executable unlinked | The old dentry path with the literal suffix ` (deleted)`. The suffix is explicitly documented and is ambiguous because a real filename may end with those characters. |
| Old path atomically replaced by another file | The running image remains tied to the old, now-unlinked dentry, so it reports the old path plus ` (deleted)`, not the replacement. |
| Executed hard-link name unlinked while another hard link survives | Still reports the executed name plus ` (deleted)`; the kernel does not search for a surviving hard-link name. |
| Parent directory removed | A nonempty parent cannot normally be removed. After unlinking the executable and removing now-empty ancestors, the retained executable dentry is still unlinked and retains the deleted-path presentation while references survive. |
| Current directory changed | No effect. The result comes from the executable file and process root, not `argv[0]` plus the current cwd. |
| Root/mount namespace changed, mount detached, or path otherwise unreachable | The kernel renders from its retained path and the current process root/mount view. Such output is not guaranteed to be openable from another namespace or after namespace changes. |

The Linux manual explicitly documents the normal path, the ` (deleted)`
suffix, the ptrace-based permission check, and one additional failure: in a
multithreaded process the link is unavailable after the main thread has
terminated
([`proc_pid_exe(5)`](https://man7.org/linux/man-pages/man5/proc_pid_exe.5.html)).
Rust also fails if procfs is absent, the readlink is denied, or the kernel
cannot render the path. Rename and ancestor-rename behavior is
implementation-derived from the current `d_path` walk; the replacement,
hard-link, and removed-ancestor rows are inferences from that retained
dentry/path mechanism rather than separate man-page promises.

## macOS / Darwin

Rust uses `_NSGetExecutablePath`, not `proc_pidpath`. Apple's API explicitly
says it returns “a path,” not a real path, and that the result may be a symbolic
link
([`dyld(3)`](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/dyld.3.html)).

Modern dyld makes the post-launch behavior clear in source. On macOS,
`_NSGetExecutablePath` copies `mainUnrealPath`
([`DyldAPIs.cpp`](https://github.com/apple-oss-distributions/dyld/blob/main/dyld/DyldAPIs.cpp)).
That value is established during dyld process initialization from the
kernel-provided `executable_path`, falling back to `argv[0]`, and a relative
value is made absolute with the startup cwd
([`DyldProcessConfig.cpp`](https://github.com/apple-oss-distributions/dyld/blob/main/dyld/DyldProcessConfig.cpp)).
It is retained process state, not recomputed by looking up the file.

| Circumstance | `_NSGetExecutablePath` / Rust result |
| --- | --- |
| Normal call | The absolute launch path retained by dyld. |
| Invoked through a symlink | The symlink spelling may be retained; no `realpath` is performed by this API. |
| Executable renamed/moved | The unchanged launch path, now stale. |
| Ancestor renamed/moved | The unchanged launch path, now stale. |
| Executable or ancestor removed | The unchanged launch path; later nonexistence does not itself make `_NSGetExecutablePath` fail. |
| Old path replaced | The unchanged string may now name the replacement, not the executing vnode. |
| Current directory changed | No effect, because a relative launch value was expanded during dyld initialization. |

Apple documents the non-real-path/symlink property. The mutation rows are
implementation-derived from the cited current dyld sources, not explicit
stability promises in the `_NSGetExecutablePath` manual page.

Darwin also has `proc_pidpath`, but it is a different semantic. XNU obtains the
process text vnode, derives its current path, then verifies that the path still
looks up; failures such as removal propagate as errors
([XNU `proc_pidpathinfo_internal`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/proc_info.c)).
Rust intentionally does not use that facility for `current_exe`.

## BSDs and other Unix variants

FreeBSD illustrates why even vnode-based Unix APIs need a weak contract.
`KERN_PROC_PATHNAME` is documented as returning the path of the process text
file
([FreeBSD `sysctl(3)`](https://man.freebsd.org/cgi/man.cgi?query=sysctl&sektion=3)).
The current kernel first tries to reconstruct the same hard-link spelling used
by `execve`, then looks that path up and confirms it is still the executable
vnode. If it was renamed or replaced, it falls back to `vn_fullpath`
([FreeBSD `proc_get_binpath`](https://github.com/freebsd/freebsd-src/blob/main/sys/kern/kern_proc.c)).
That fallback is best effort and name-cache based. It can find a current name
after a rename, choose a different available hard-link name, or fail after an
unlink; FreeBSD does not promise Linux's ` (deleted)` representation.

Other examples visible in Rust's implementation are enough to rule out one
generic policy:

- NetBSD prefers a pathname sysctl and falls back to procfs.
- OpenBSD's Rust implementation is `argv[0]` based. A path-like value is
  canonicalized on every call and can therefore fail after rename/removal,
  while a bare name is returned bare without a filesystem search.
- Solaris/illumos prefers a procfs link but falls back to `getexecname`, whose
  relative result is joined with the cwd at call time.
- AIX searches the current `PATH`/cwd and canonicalizes at call time, so
  environment and filesystem mutation can change the result or make it fail.

These are API implementation facts, not a promise that all releases of those
systems have identical mutation edge cases.

## Recommended moonrun contract

Moonrun must not call the host's native current-executable API: that returns
the moonrun process, not the guest wasm module. The guest contract should be:

1. A file-backed module's current executable path is the path by which
   `Engine::load_file` loaded that module, made absolute against the cwd at
   load time with `std::path::absolute`.
2. Do not canonicalize it, resolve symlinks, re-stat it, or recompute it at
   each run/call. Preserve this value for all runs of that `Module`.
3. The result is a path name only. It may be stale, nonexistent, or point to a
   different object after filesystem mutation. Cwd changes do not affect it.
4. `Engine::compile(name, bytes)` creates an in-memory module. Its `name` is a
   diagnostic label, not a path; `current_exe` must report an error unless a
   separate API explicitly supplies a file origin.
5. If the path cannot be made absolute or represented as a MoonBit string,
   preserve that failure and report it to the guest. On Unix this includes a
   non-UTF-8 path if the MoonBit API requires a string. On Windows the native
   UTF-16 units can be copied directly.
6. For linear wasm, `moonbitlang/core` should return a buffer handle allocated
   through the existing `moonbitlang/async` host resource table. Return the
   invalid handle and set async errno on failure. The guest copies the UTF-16
   units into a MoonBit `String` and frees the handle using the existing async
   C-buffer API.

`std::path::absolute` is specifically suitable because it makes a path
absolute without canonicalizing through the filesystem; its precise lexical
behavior remains platform-aware
([Rust documentation](https://doc.rust-lang.org/std/path/fn.absolute.html)).

## Moonrun implementation

The implementation keeps the display `name` and executable origin as separate
`ModuleData` fields. `Engine::load_file` captures an absolute, lexical
`runtime::Executable` once, while `Engine::compile(name, bytes)` stores an
unavailable origin because its name is only diagnostic. Each run clones that
value into the backend-neutral `Runtime`.

The `moonbitlang/core` adapter reads the executable through `Runtime`, converts
its path to MoonBit UTF-16, allocates the result in the existing async C-buffer
table, and records async errno when the origin or encoding is unavailable. The
V8 context retains only the `Runtime`; it neither derives nor owns executable
state. Consequently, cwd changes between module loading and execution do not
change the result, and non-UTF-8 Unix paths reach the intended guest-visible
error path without passing through the lossy diagnostic name.

Tests should cover relative file loading followed by a cwd change, module reuse
after a rename/unlink, symlink spelling preservation, in-memory module failure,
non-UTF-8 Unix failure, Windows wide-string preservation, errno propagation,
and buffer release.
