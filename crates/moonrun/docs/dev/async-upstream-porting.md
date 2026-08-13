# Porting `moonbitlang/async` changes

Moonrun implements the wasm host side of `moonbitlang/async`. Upstream changes
must be audited against the Native-Shaped Async Boundary instead of being
accepted by updating the submodule and making provenance checks pass.

## Port queue

The labels are a one-way communication channel from `moonbitlang/async` to
Moonrun. An async pull request that requires a Moonrun audit or backport is
marked with the upstream
[`wasm-runtime-port-required`](https://github.com/moonbitlang/async/issues?q=state%3Aopen%20label%3Awasm-runtime-port-required)
label. Moon consumes that queue; Moon pull requests do not use the label to
publish their own state. Do not use mentions as a substitute for the label.
Also search merged and closed pull requests carrying it: GitHub allows a
merged pull request to retain its labels, so the open queue is not a complete
audit history.

Do not self-apply either label to a coordinating async pull request opened as
part of a Moon port. Such a pull request is an implementation dependency of
the port, not a new upstream request to Moon. Reference the paired Moon pull
request and state the merge order instead. Only update labels on the
originating async pull request that placed work in the queue.

On an async pull request, the labels mean:

- `wasm-runtime-port-required`: async is telling Moon that this upstream change
  still needs runtime work or investigation.
- `wasm-runtime-port-completed`: Moon has consumed that request because the
  behavior is available on Moon's `main`, or an audit proved that Moon already
  implements it and no change is necessary.

A partial port, local branch, or open Moon pull request remains
`wasm-runtime-port-required`. After the corresponding change lands on Moon's
`main`, replace that label on the upstream pull request with
`wasm-runtime-port-completed`. If any guest or runtime work remains, keep the
required label and record the gap in the Moon pull request. If a later async
pull request introduces another runtime requirement, label that pull request
independently when it is an upstream-originated request, even when the earlier
request is already completed.

## Separate discovery from delivery

Audit an upstream update before preparing the commits that will deliver it:

1. Start from the latest Moon `main` and temporarily update
   `third_party/moonbitlang_async` to async's latest `main`.
2. Inspect the complete upstream commit range and every pull request carrying
   `wasm-runtime-port-required`.
3. Run the provenance checks before changing annotations. A missing source or
   symbol is an audit signal, not a formatting failure.
4. Compare the native C implementation together with every target-specific
   MoonBit wrapper, especially the wasm imports, to Moonrun's Async API, Async
   Host, and Async Sys behavior. Do not assume an unchanged wasm wrapper
   already represents the new native ABI.
5. If the wasm wrapper still uses a replaced ABI, prepare the coordinating
   async guest pull request during discovery. Compile that guest and audit its
   complete import closure, including auxiliary imports initialized by shared
   code, before finalizing the Moon implementation.
6. Classify each change as an exact move, an already-matching behavior change,
   or a required port. Do not commit the audit bump while it has unresolved
   provenance or behavior changes.
7. Treat the port as complete only when the upstream tests run against the
   updated guest wrapper through the Moon change. Passing those tests with the
   old compatibility wrapper does not exercise the new ABI.

Deliver required ports one upstream pull request at a time. Restore the pinned
submodule when necessary, implement the behavior and regression coverage in a
focused change, and keep that change independently buildable and testable. A
submodule bump belongs with the port that makes its affected provenance and
behavior accurate; it is not evidence that the port is complete by itself.
When the port also needs a coordinating async guest change:

1. Open the unlabeled async pull request while developing the Moon port.
2. Pin its reachable commit for end-to-end testing and include its complete
   runtime requirements in the same Moon pull request that consumes the
   originating queue item.
3. Merge the Moon pull request first, so the runtime accepts the new guest.
4. Merge the coordinating async pull request only after the Moon change is on
   `main`.

Do not defer a guest-visible import that compiling the coordinating guest
could have revealed to a second Moon pull request. Such a follow-up means the
original audit stopped before closing the end-to-end ABI boundary.

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
- If compatibility only translates a historical Wasm ABI before calling the
  current ported Async Sys implementation, record it as an `api_only = true`
  `#[compat]` adapter. Do not add a duplicate Async Sys wrapper solely for
  provenance.
- For a replacement, audit the new ABI, ownership, errors, platform behavior,
  and observable MoonBit API semantics. Add the new `#[ported]` implementation
  separately from the compatibility adapter.

The annotations must continue to expose unported upstream work. A passing
substring check is not proof that two symbols are equivalent.

## Wasm ABI discipline

Treat every wasm import as an exact typed interface. The current V8 adapter
uses JavaScript callbacks internally, but imports must also link correctly in a
runtime such as Wasmtime that validates the complete function type.

- Do not multiplex multiple arities under one import name.
- Do not rely on missing arguments, ignored extra arguments, or JavaScript
  value coercion.
- Declare every field produced by MoonBit aggregate lowering. If a field is
  constrained by the operation, validate the constraint instead of silently
  discarding the field.
- When an operation needs a new signature, add a distinct import and retain the
  old import as `#[compat]` while older wasm guests remain supported.
- Record and test the exact parameter and result types of the old and new
  imports. Compatibility must be decided at link time, before a mismatched call
  can produce host side effects.

## Verification

Add regression coverage for behavior that upstream tests restrict to native
targets. Run the Moonrun test and lint checks, then run the synced async wasm
test suite against the built Moonrun when guest-visible behavior or the async
boundary changed.
