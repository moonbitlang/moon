---
name: moon-development
description: Route implementation, design, review, diagnosis, and testing work on the Moon CLI and MoonBuild to canonical glossary, ADR, design, behavioral-reference, and testing documents. Use when work touches crates/moon, crates/moonbuild-rupes-recta, crates/moonbuild, crates/moonbuild-debug, crates/mooncake, crates/moonutil, xtask, or docs for moon commands, package discovery, dependency resolution, workspaces, build planning and lowering, targets and toolchains, native builds, command output, caching, moon run, moon test, moonx, or runwasm. Do not use for crates/moonrun runtime internals.
---

# Moon Development

Use progressive disclosure to load only the Moon and MoonBuild context relevant
to the change. Do not read or reproduce the entire developer-document corpus.

## Route the work

1. Read `CONTEXT.md` before naming, changing, or reviewing MoonBuild concepts,
   command communication, package execution, or native-build behavior. Use its
   canonical terms and respect its avoided synonyms.
2. Open `docs/dev/README.md`. Use its task table to select the relevant ADRs,
   designs, implemented-behavior references, migration notes, and testing
   guidance.
3. Read the selected documents together with the affected source and tests.
   Do not assume a design or migration document describes implemented behavior.
4. Establish which build engine owns the changed path. Rupes Recta is the
   default; inspect legacy `moonbuild` only when the path still uses it or the
   requirement includes compatibility.
5. Surface conflicts with an ADR or behavioral reference explicitly. Do not
   silently replace an existing decision or leave an implemented-behavior
   reference stale.
6. Route `crates/moonrun` runtime-internal work through its own `CONTEXT.md` and
   developer index rather than treating it as MoonBuild implementation.

## Deepen before layering

Before adding a MoonBuild planning or lowering type, projection, or seam:

- Identify the closest existing module and prefer deepening, moving, or
  extending it over adding a parallel representation.
- Apply the deletion test to the proposed module. If deleting it would
  concentrate its rules in an existing owner instead of scattering them among
  callers, do not introduce it.
- State the unique invariant owned by a new representation and why a real seam
  needs it. Overlapping fields or a single fixed adapter do not justify one.
- Design the merged end state before splitting the work. Intermediate commits
  may carry compatibility code, but the PR should replace or materially shrink
  the old model rather than merge two peer models into the default branch.
- Mark every intentionally retained compatibility representation, known seam
  violation, or deferred deletion in source with a searchable `FIXME` or
  `TODO` that states the removal condition. Ordinary comments, design
  documents, and PR descriptions do not replace the source marker.
- Separate semantic changes from optional terminology cleanup. A rename is not
  an architectural benefit.

## Keep each fact in one home

- Put domain vocabulary in `CONTEXT.md`.
- Put accepted, hard-to-reverse trade-off decisions in `docs/adr/`.
- Put cross-cutting designs and invariants in `docs/dev/design/`.
- Put implemented MoonBuild behavior in `docs/dev/reference/` and update it
  with the implementation.
- Put migration sequencing in its named migration document.
- Put user-facing command behavior in `docs/manual/` and `docs/manual-zh/`.
- Treat source and tests as the final evidence for the current revision.
- Update `docs/dev/README.md` when adding, renaming, superseding, or changing
  the required audience of a developer document.

Avoid copying the task-to-document table into this Skill. The developer index
owns the route; linked documents own the facts.

## Validate proportionally

Use the task route in the developer index instead of prescribing one universal
command. Preserve behavior across commands and both build engines when the
feature applies to both. Prefer the narrowest test surface that proves the
changed phase, and add end-to-end coverage only when the behavior crosses
command or process seams.
