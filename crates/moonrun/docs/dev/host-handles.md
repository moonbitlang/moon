# Host handles

[ADR 0009](../adr/0009-use-one-handle-namespace.md) records the decision to share
one Handle namespace. This document describes the shared Handle contracts and
ownership rules that span host objects.

## Registration and validation

The Runtime owns `HostKeys`, a shared `SlotMap<HostKey, HostResourceKind>` that
allocates all guest-visible Handles. Each Host Domain owns its payload tables,
keyed by `HostKey`. Lookup-only structures, such as the Windows OVERLAPPED pointer
map, are secondary indexes back to Handles. Import registries declare ABI and
register callbacks; they do not own keys or payloads.

Decoding a guest integer produces a candidate `HostKey`. Lookup validates its
generation and expected `HostResourceKind` in `HostKeys`, then checks membership
in the owning payload table. Operations convert missing entries to their ABI
error, with explicit special cases where an operation accepts an invalid Handle
as a no-op.

The invalid Resource Handle returned by `invalid_fd()` names a preallocated
`Resource::invalid()` entry. It has a live key of kind `Resource`; Resource
lookup rejects its invalid payload, and other families reject its kind.
Optional Resource arguments, such as process stdio and PID-only waits, use
this reserved Handle to express absence. All other values undergo the ordinary
Resource lookup or process-ownership check.

Nullable host objects use the encoded `HostKey::null()` returned by
`runtime::null_handle()`. Slotmap guarantees that this key is always invalid and
distinct from every live key. It occupies no table entry. Null buffers, including
core executable-path and TLS results, and address-info list terminators use this
sentinel. Buffer and address-info wrappers recognize it through host null
predicates; SQLite exposes its value through `sqlite3_null_handle`.

Operations that accept null retain their operation-specific behavior, such as
freeing a null buffer as a no-op. An operation that requires a live object
validates it through its owning table. Zero is subject to ordinary validation.
These Handle values are separate from raw OS descriptors, where zero can be valid.

Host TLS owns its connection table. Creation returns a live Handle containing
pending configuration.
Client and server setup keep the same Handle. Initialization failures retain the
pending configuration and report the failure through their status and the object's
error message. Freeing a TLS connection accepts the shared null Handle as a no-op;
zero fails Handle validation with `Badf`.

## Owning tables

All opaque Handle tables use `Handles<T>` to own their values together with their
Host Key registrations. `HostKeys` exposes no allocation or retirement operations
to domains; its mutable registry is private to `runtime::handles`.
Insertion accepts a complete value and allocates its key. Lookup checks both the
central registration and membership in this table. Removal retires the key and
returns the value so callers can release the table borrow before performing
domain cleanup.

Dropping a table retires its remaining keys under a short allocator borrow.
That borrow ends before value destructors run.

## Operation ownership

`async_host::buffers` owns guest-visible buffer registrations and access.
Publishing an operation's result creates a Handle; leasing temporarily transfers
an already-published buffer to an operation. Both can apply to the same object.
Buffer availability describes access, not whether its contents are a completed
result. Jobs and I/OResults own completion and cleanup timing; their private
storage does not need a separate buffer registration.

A Job or I/OResult may own buffer bytes while the table retains a leased entry.
Freeing the Handle retires its registration and removes that reservation; the
operation retains ownership of the bytes until its cleanup completes. A returned
lease restores the bytes only if the original entry still exists and is leased.
Generation checks prevent a late return from restoring a freed entry or
overwriting a replacement Handle.

Finishing native work does not itself restore guest access: the operation owner
must first collect or finalize its result. Cancellation alone does not permit
returning the storage.

The `poll` module owns Poll registrations and the current readable event batch.
Resource close and thread-pool teardown unregister through that owner. Removing
a Poll retires its Handle and event access before native destruction; outstanding
Windows requests and completion senders retain their own port references.

Windows I/OResults own their Handle registrations, stable `OVERLAPPED` storage,
buffers, and acquired Resources in `async_host::windows_io`. Pending requests
also retain their completion port after its Poll Handle is destroyed. Polling
resolves native completion addresses once into generational keys; reading a
cached event cannot complete a later submission or identify a replacement Handle.
Cancellation retains requests until completion, and shutdown cancels all requests
before draining them and releasing their storage.

Unix process launch validates its argument and environment inputs before
consuming either Handle. Transferring an environment block validates the
destination Handle before consuming the source; insufficient destination
capacity still consumes that temporary source snapshot. The Process domain
owns these input tables and transfer rules on the calling thread. It snapshots
the shared Runtime Env when inputs are created; Jobs receive completed values
after transfer. Each owning table manages registration and retirement of its
entries.

Resource operations validate the Resource Handle and the Resource Class required
by the operation before calling into the OS. Files, TCP sockets, and UDP sockets
share the namespace; their operation permissions remain domain concerns.
Jobs acquire host-owned Resource references before blocking work, so removing a
guest Handle does not invalidate a Resource already acquired by a Job.
The Resource table also owns the reserved invalid and stdio registrations.
Aliased stdio streams share one Handle; reserved Handles cannot be closed by the
guest and retire with the table without closing Runtime-owned stdio.

SQLite retains its own native cleanup rules: a busy Database close preserves its
Handle, while Statement finalization retires the Handle even when SQLite reports
an execution error. Runtime teardown releases guest mutex entries before joining
Workers, then SQLite retires and destroys Statements before Databases. Owning
tables do not replace Job leases or choose native destruction order.

The Thread Pool owns Worker Handles. Waking a Worker validates its Handle
before transferring a Job. Freeing or destroying Workers retires their
Handles before cancellation and joining; queued Jobs return to Async Host for
restoration, alongside completed results. Callers do not manipulate the Worker
table or join native threads directly.

Workers can create OS Resources, but publishing guest-visible Handles belongs
to the guest-thread event loop. An open Worker returns a completed Job with an
unpublished Resource; the Async Host publishes its Handle when the result is
observed. Worker threads do not mutate the Handle tables.

Payload storage, operations, and leak accounting stay with their owning domains.
When leak checking is enabled, the Runtime combines domain summaries as it is
dropped. An individual import context does not inspect another domain's keys.
