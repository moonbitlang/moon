# Dependency-tree deduplication in Cargo and Go

Verified against current official documentation and first-party source on
2026-08-27.

## Recommendation

Use Cargo's human-output policy as the precedent for `moon tree`:

- Expand a dependency's children at its first displayed occurrence.
- Keep every later parent-to-dependency occurrence visible, but omit the
  already displayed children.
- Put the omission marker on the repeated dependency line, rather than adding
  a synthetic child such as `(already shown)`.
- Do not mark a repeated leaf, because no subgraph was omitted.
- Add `--no-dedupe` with the behavior change so users can recover the complete
  path-expanded view. Do not call this flag `--duplicates`: Cargo uses
  `--duplicates` for a different question, packages resolved at multiple
  versions.
- Keep Moon's explicit `(cycle)` marker distinct from ordinary deduplication.

For the compact marker, Cargo's `(*)` is familiar and cheap. A more explicit
`(already shown)` is also defensible, but it should be appended to the repeated
dependency itself:

```text
alice/root:
├─ alice/a
│  └─ alice/shared
│     └─ alice/leaf
└─ alice/b
   └─ alice/shared (*)
```

The Moon implementation follows this policy: it uses `(*)` on repeated
non-leaves, leaves repeated leaves unmarked, and provides `--no-dedupe` for
complete path expansion. Its per-root deduplication scope is a deliberate
difference from Cargo.

## Cargo

`cargo tree` is a nested, human-oriented dependency view. By default, when a
package's dependencies have already been shown, later occurrences still print
the package under each parent, append `(*)`, and do not print its dependencies
again. `--no-dedupe` restores repeated expansion. The Cargo Book both defines
this behavior and shows it in the default output example. [Cargo tree
description](https://doc.rust-lang.org/cargo/commands/cargo-tree.html#description),
[Cargo tree options](https://doc.rust-lang.org/cargo/commands/cargo-tree.html#tree-options)

Cargo's implementation makes the traversal scope precise. It creates one
`visited_deps` set before iterating over all displayed roots; the first
`insert(node)` succeeds and recursion proceeds, while a later occurrence
returns before its dependencies are visited. Deduplication therefore spans the
whole displayed forest, not only one root or one ancestor path. [Cargo tree
implementation, traversal and visited
set](https://doc.rust-lang.org/stable/nightly-rustc/src/cargo/ops/tree/mod.rs.html#266-363)

Cargo suppresses `(*)` for a repeated node with no outgoing edges, explicitly
because no dependency subtree was deduplicated and the marker would be noise.
The leaf occurrence itself is still printed under every parent. [Cargo tree
implementation, marker
selection](https://doc.rust-lang.org/stable/nightly-rustc/src/cargo/ops/tree/mod.rs.html#343-356)

With `--no-dedupe`, Cargo still uses a separate recursion stack to stop cycles;
full repeated expansion does not imply infinite traversal. [Cargo tree
implementation, cycle stack](https://doc.rust-lang.org/stable/nightly-rustc/src/cargo/ops/tree/mod.rs.html#266-363)

Cargo's `-d` / `--duplicates` is unrelated to display deduplication. It selects
packages present at multiple versions and implies `--invert`; with `-p`, it
limits that search to a package subtree. [Cargo tree
options](https://doc.rust-lang.org/cargo/commands/cargo-tree.html#tree-options)

For programs, Cargo exposes a normalized graph through
`cargo metadata --format-version=1`: `resolve.nodes` contains one entry per
package, and dependencies are package-ID references. A shared package is thus
one graph node reached by multiple references rather than a recursively nested
object repeated at each path. [Cargo metadata JSON
format](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html#json-format)

## Go

Go's relevant built-in commands do not provide a Cargo-like nested dependency
tree, so Go does not establish a marker or `--no-dedupe` convention. This is an
inference from the documented shapes of the built-in inspection commands:

- `go mod graph` prints the module requirement graph as an edge list, one
  `module dependency` pair per line. Shared modules naturally recur as edge
  endpoints; no subtree is embedded or repeated. [Go Modules Reference: `go
  mod graph`](https://go.dev/ref/mod#go-mod-graph)
- `go mod why` answers a narrower question: for each requested package or
  module, it prints one shortest package-import path from the main module. It
  does not render the full graph as a tree. Even with `-m`, it queries the
  package graph rather than the module graph from `go mod graph`. [Go Modules
  Reference: `go mod why`](https://go.dev/ref/mod#go-mod-why)
- `go list -deps` emits a flat depth-first post-order sequence of package
  records. `-json` changes each record's representation, while the `Imports`
  and `Deps` fields expose direct and recursive dependency names; neither form
  introduces a nested-tree dedup marker. [Official `go list`
  documentation](https://pkg.go.dev/cmd/go#hdr-List_packages_or_modules)
- `go list -m all` lists the selected module build list, and
  `go list -m -json all` provides flat module records for tools. Neither is a
  dependency hierarchy. [Go Modules Reference: `go list
  -m`](https://go.dev/ref/mod#go-list-m)

The design lesson from Go is therefore about representation rather than tree
formatting: when exact graph structure matters, expose edges or flat node
records instead of path-expanded nested objects. It does not argue for or
against Cargo's human-tree policy.

## Implications for Moon

Moon's JSON representation already follows the graph-oriented model. Module
JSON builds one indexed `modules` array and a separate `edges` array; package
JSON deduplicates nodes by `PackageId`, stores them once, and preserves shared
relationships as edges. The regression test also checks that a shared
module appears once while two edges point to it. [Moon module JSON
renderer](../../../crates/moon/src/cli/tree.rs#L376), [Moon package JSON
renderer](../../../crates/moon/src/cli/tree.rs#L462), [shared-subgraph JSON
test](../../../crates/moon/src/cli/tree.rs#L1009)

Consequently, `--no-dedupe` should affect text output only. Applying it to JSON
would either be meaningless or would replace the graph schema with a lossy,
path-expanded structure.

The human renderer has two independent sets with distinct roles: a recursion
`stack` for cycles and an `expanded` set for shared subgraphs. It keeps Moon's
explicit `(cycle)` child while appending `(*)` to a repeated dependency whose
children are omitted. [Moon human tree
renderer](../../../crates/moon/src/cli/tree.rs#L745)

Moon initializes `expanded` once for the single module root but once *per
source-package root* in `moon tree --package`. Cargo instead shares
one visited set across all roots. Per-root scope keeps every selected Moon
package independently inspectable, while command-wide scope maximizes output
reduction. Either can work, but the manual must state the choice; the current
wording, “once per root,” is an intentional divergence from Cargo. [Moon
package-tree roots](../../../crates/moon/src/cli/tree.rs#L559), [Cargo's shared
visited set](https://doc.rust-lang.org/stable/nightly-rustc/src/cargo/ops/tree/mod.rs.html#266-295)

One Moon-specific identity issue also needs an explicit choice. Package text
tracks expansion by `BuildTarget`, while its visible label identifies only the
package; package JSON instead deduplicates by `PackageId` and aggregates target
kinds on edges. If two target kinds for one package have different outgoing
edges, deduplicating them by visible package label would hide information, but
keeping `BuildTarget` identity can show apparently repeated package subgraphs.
The human output should either display target kind when it is identity-relevant
or document that identical-looking package labels may denote different build
targets. [Moon package text child
identity](../../../crates/moon/src/cli/tree.rs#L606), [Moon package JSON edge
aggregation](../../../crates/moon/src/cli/tree.rs#L462)

## Scope boundary

Default text deduplication, its marker behavior, leaf behavior, cycle behavior,
and `--no-dedupe` belong together because they define one user-visible output
policy and its escape hatch. A later change can alter the multi-root scope or
redesign package-text identity; those choices need separate examples and
compatibility discussion.
