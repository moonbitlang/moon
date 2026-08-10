# Use a Stable Host-Owned Generic Stat Result

Moonrun ports the generic filesystem metadata ABI introduced by
`moonbitlang/async` PR 527 through one `StatRequest`/`StatValues`/`PackedStat`
module. Jobs retain the request and produce a host-owned packed result; they
never retain the guest destination supplied to `make_open_job`,
`make_fstatx_job`, or `make_statx_job`. The guest copies a completed result
through `thread_pool/get_stat_result`, following the same completion ownership
rule as read jobs.

Open jobs use one completion shape: an `OpenJobResource` plus the host-owned
`PackedStat`. The pre-527 open adapter requests the fixed identity mask and
serves its legacy getters from that packed result; it does not maintain a
second set of kind, device, and file-id fields.

The packed format is stable for a given request: an eight-byte little-endian
header contains the result length and returned-property mask, followed by fixed
slots in property-bit order. Unsupported properties keep their slots zeroed
and are omitted from the returned mask. Unknown request bits and undersized
buffers produce a failed Job before filesystem side effects.

Platform adapters return typed `StatValues`; only the shared codec writes the
ABI representation. Linux uses `statx` directly, macOS uses the selective
`getattrlist` family with the native non-vnode fallback, and Windows performs
selective handle queries. Windows timestamps are converted from FILETIME's
1601 epoch to the Unix epoch required by the cross-platform contract.

Metadata access remains subject to the runtime filesystem policy. Handle
queries require metadata-read authority for the acquired Resource, and path
queries use the parent Resource as their policy base. Open jobs continue to use
the open policy for the identity mask (kind, device id, and file id); requesting
additional metadata also requires metadata-read authority.

The pre-527 imports remain available for older wasm guests, but their
implementations use `#[compat]` provenance that names PR 527 and the generic
replacement. They must not be relabeled as `#[ported]`, because their original
upstream symbols no longer exist. The current generic imports use exact
`#[ported]` provenance.
