# Host handles

[ADR 0009](../adr/0009-use-one-handle-namespace.md) records the decision to share
one Handle namespace. This document describes its implementation and the
ownership rules used by host operations.

## Registration and validation

The Runtime owns `HostKeys`, a shared `SlotMap<HostKey, HostResourceKind>`.
Guest-visible Handles for Resources, Jobs, Workers, poll instances, Host Buffers,
address-info results, SQLite objects, and other host objects are allocated from
this table. Payloads live in family-specific `SecondaryMap<HostKey, Payload>`
tables. Lookup-only structures, such as the Windows OVERLAPPED pointer map, are
secondary indexes back to Handles. Import registries declare ABI and register
callbacks; they do not own keys or payloads.

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

TLS connection creation returns a live Handle containing pending configuration.
Client and server setup report initialization failures through their status and
the object's error message. Freeing a TLS connection accepts the shared null
Handle as a no-op; zero fails Handle validation with `Badf`.

## Owning tables

`Handles<T>` owns C buffers, address-info results, process argument arrays,
environment blocks, environment builders, and TLS connections together with
their Host Key registrations. Insertion accepts a complete value and allocates
its key. Lookup checks both the central registration and membership in this table.
Removal retires the key and returns the value so callers can release the table
borrow before performing domain cleanup.

Dropping a table retires its remaining keys under a short allocator borrow.
That borrow ends before value destructors run. Domain state such as a leased
buffer remains an entry until its Handle is freed, even while a Job owns the
bytes.

A TLS entry keeps its Handle when its pending configuration becomes a configured
connection. Configuration failure retains the pending entry and its error;
successful configuration replaces its value without changing its key.
Freeing the Handle retires its key before dropping the TLS value outside the
table borrow. Host teardown also retires all remaining TLS keys and drops their
values.

Other families coordinate their payload tables and Host Key registrations
directly. Central validation prevents access to a secondary entry whose key has
already been retired. Generation checks prevent a stale Handle from accessing
a replacement entry.

## Operation ownership

Unix process launch validates its argument and environment inputs before
consuming either Handle. Transferring an environment block validates the
destination Handle before consuming the source; insufficient destination
capacity still consumes that temporary source snapshot. The Async Host owns
these operation-specific rules, while each owning table manages registration
and retirement of its entries.

Resource operations validate the Resource Handle and the Resource Class required
by the operation before calling into the OS. Files, TCP sockets, and UDP sockets
share the namespace; their operation permissions remain domain concerns.
Jobs acquire host-owned Resource references before blocking work, so removing a
guest Handle does not invalidate a Resource already acquired by a Job.

Workers can create OS Resources, but publishing guest-visible Handles belongs
to the guest-thread event loop. An open Worker returns a completed Job with an
unpublished Resource; the Async Host publishes its Handle when the result is
observed. Worker threads do not mutate the Handle tables.

Payload storage, operations, and leak accounting stay with their owning domains.
When leak checking is enabled, the Runtime combines domain summaries as it is
dropped. An individual import context does not inspect another domain's keys.
