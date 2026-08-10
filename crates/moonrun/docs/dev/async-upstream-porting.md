# Porting `moonbitlang/async` changes

Moonrun implements the wasm host side of `moonbitlang/async`. Upstream changes
must be audited against the Native-Shaped Async Boundary instead of being
accepted by updating the submodule and making provenance checks pass.

## Port queue

Use the upstream
[`wasm-runtime-port-required`](https://github.com/moonbitlang/async/issues?q=state%3Aopen%20label%3Awasm-runtime-port-required)
label as the incoming queue. Do not use mentions as a substitute for the
label. Also search merged and closed pull requests carrying the label: GitHub
allows a merged pull request to retain it, so the open queue is not a complete
audit history.

The workflow labels mean:

- `wasm-runtime-port-required`: runtime work or investigation is still needed.
- `wasm-runtime-port-completed`: the behavior is available on Moon's `main`, or
  an audit proved that Moon already implements it and no change is necessary.

A partial port, local branch, or open Moon pull request remains
`wasm-runtime-port-required`. After the corresponding change lands on Moon's
`main`, replace that label on the upstream pull request with
`wasm-runtime-port-completed`. If any guest or runtime work remains, keep the
required label and record the gap in the Moon pull request.

## Separate discovery from delivery

Audit an upstream update before preparing the commits that will deliver it:

1. Start from the latest Moon `main` and temporarily update
   `third_party/moonbitlang_async` to async's latest `main`.
2. Inspect the complete upstream commit range and every pull request carrying
   `wasm-runtime-port-required`.
3. Run the provenance checks before changing annotations. A missing source or
   symbol is an audit signal, not a formatting failure.
4. Compare the native C implementation together with its MoonBit wrapper to
   Moonrun's Async API, Async Host, and Async Sys behavior.
5. Classify each change as an exact move, an already-matching behavior change,
   or a required port. Do not commit the audit bump while it has unresolved
   provenance or behavior changes.

Deliver required ports one upstream pull request at a time. Restore the pinned
submodule when necessary, implement the behavior and regression coverage in a
focused change, and keep that change independently buildable and testable. A
submodule bump belongs with the port that makes its affected provenance and
behavior accurate; it is not evidence that the port is complete by itself.

## Provenance annotations

`#[ported]` records the exact upstream source path and symbol tracked by an
implementation. `#[compat]` records a retained ABI whose upstream implementation
or import was removed or replaced, including the upstream PR and its
replacement. It describes the provenance of the adapter, not whether the
currently pinned wasm wrapper still calls it during a staged migration.

- If a symbol only moved to another file, update `source` after confirming that
  its signature and behavior did not change.
- If a symbol was removed, replaced, or generalized, do not point `original`
  at a surviving successor merely to satisfy the provenance test. Preserve it
  as `#[compat]` when older wasm guests still need the ABI.
- If a removed import intentionally does no work because ownership has already
  transferred, record it as a no-op `#[compat]` adapter instead of a generic
  helper.
- For a replacement, audit the new ABI, ownership, errors, platform behavior,
  and observable MoonBit API semantics. Add the new `#[ported]` implementation
  separately from the compatibility adapter.

The annotations must continue to expose unported upstream work. A passing
substring check is not proof that two symbols are equivalent.

## Verification

Add regression coverage for behavior that upstream tests restrict to native
targets. Run the Moonrun test and lint checks, then run the synced async wasm
test suite against the built Moonrun when guest-visible behavior or the async
boundary changed.
