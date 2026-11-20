# Rupes Recta Special Cases

Rupes Recta keeps a growing list of hard-coded behaviors so the new build
pipeline remains byte-for-byte compatible with the legacy engine. The handlers
live throughout `crates/moonbuild-rupes-recta`, but this page summarizes the
important ones and why they exist.

## Self coverage

[`special_cases::self_coverage`](../../../crates/moonbuild-rupes-recta/src/special_cases/self_coverage.rs)
owns the exception for instrumenting builtin and the coverage runtime itself.
The coverage runtime depends on builtin, so builtin cannot obtain that runtime
through an ordinary import without creating a cycle. Three rules work together:

- **Builtin borrows coverage sources.** With coverage enabled and core as the
  sole project root, `augment_builtin_sources` appends coverage's source paths
  to builtin's in-memory file set and sorts it. No files are copied or rewritten,
  and the coverage package retains its own sources. Resolution applies this
  after scanning and before solving imports, so target selection and metadata
  continue to see the augmented file set. Ordinary projects consuming installed
  core do not apply this rule. Missing builtin or coverage packages retain the
  existing warning-and-skip behavior.
- **Self-coverage packages do not import coverage implicitly.** Package
  resolution uses `uses_self_runtime` to exclude builtin and coverage from
  implicit coverage dependencies. Other packages receive those edges for
  Source, InlineTest, WhiteboxTest, and BlackboxTest targets when coverage is
  enabled. This is not limited to core packages.
- **Instrumentation and collection use `@self`.** Lowering uses the same
  predicate to select the current package's runtime for compiler instrumentation
  and test-driver collection. Third-party packages are not instrumented.
  Blackbox compilation is not instrumented either, but its driver still collects
  coverage from the code under test.

The special-case module owns the source augmentation and package identities;
package resolution and lowering retain graph mutation and command generation.
Source augmentation can be removed once the compiler/core contract supplies
builtin's coverage runtime without borrowing another package's sources.

## Discovery-time preparation

- **Pin prelude imports for core blackbox tests.** Whenever discovery encounters a
  package under the core module, `add_prelude_as_import_for_core` injects
  `moonbitlang/core/prelude` into `test_imports`. This supports blackbox tests;
  whitebox and inline tests retain their package's imports. Generated drivers
  adapt prelude references to builtin operations as described in the
  [test driver core dependencies](tests.md#core-dependencies).

## Artifact path overrides

- **Stdlib bundle paths are toolchain facts.** The bundled stdlib layout
  (`_build/<backend>/release/bundle`, package `.core` and `.mi`
  files, and related paths) is owned by `moonutil::toolchain`, backed by the
  internal toolchain-layout helpers. RR may translate an already selected
  stdlib root into concrete artifact paths, but it should not rediscover the
  installed stdlib root while lowering.

## Stdlib injection boundaries

- **Stdlib injection is selected before RR lowering.** `moonbuild::compile`
  selects the stdlib root from `!is_core` and stores it in the
  `ArtifactPathResolver` carried by `CompileConfig` / `BuildMeta`.
  For ordinary projects this is the installed
  toolchain core directory. When building `moonbitlang/core` itself, it is
  `None`, so stdlib packages resolve to local `_build/...` artifacts.
- **Non-stdlib packages implicitly import `prelude` and `builtin`.** The
  solver adds both edges to every non-stdlib target unless the package already
  imports them, in which case its alias wins. Moon never passes `-std-path`, so
  installed stdlib packages reach `moonc` only as explicit `-i` imports.
  `builtin` is needed because `moonc` resolves some names re-exported by
  `prelude`, such as the `Failure` constructor, only when `builtin` is loaded.
  The implicit prelude edge uses `prelude` as its compiler-facing alias,
  reserving that alias for the core prelude. A user import or blackbox-test
  self-import named `prelude` conflicts with this implicit edge and is rejected
  by package resolution; a graceful migration for existing uses is deferred.
  Fully qualified `@moonbitlang/core/prelude.xxx`
  references in generated test drivers remain valid. The implicit builtin edge
  keeps `moonbitlang/core/builtin` as its alias, so user packages and imports
  can still use `builtin`; user code must explicitly import builtin to write
  `@builtin.xxx`. Any other stdlib package must be imported explicitly.
- **RR consumes the selected resolver.** `CompileConfig`, `LoweringContext`,
  `BuildMeta`, metadata generation, and `all_pkgs.json` generation all consume
  the same selected `ArtifactPathResolver`. `metadata.rs` and `all_pkgs.rs`
  should render paths from the supplied resolver; they should not call
  `moonutil::toolchain::core()` or otherwise decide whether prebuilt stdlib
  artifacts are in use.
- **Metadata files respect stdlib mode.** When either `packages.json` or
  `all_pkgs.json` is generated for the stdlib (core module), it is generated
  without a stdlib directory so package metadata and indirect dependency
  resolution use the locally built `.mi` files. For non-core projects, the
  stdlib directory is set so prebuilt artifacts can be used.
- **The proof prelude is independent of stdlib injection.** Every proof
  execution adds one proof prelude provider directory to the Why3 loadpath. It
  defaults to the toolchain's `lib/prelude_proof`; when
  `MOON_PROVE_PRELUDE_OVERRIDE` is set, the command layer validates and selects
  that directory instead. This belongs to command execution; RR prepares
  proof interfaces and WhyML without a solver configuration or prelude. This also
  applies when proving `moonbitlang/core`, where the installed stdlib is
  intentionally not injected; the proof prelude is a toolchain resource rather
  than a prebuilt stdlib package artifact.

## Wasm separate compilation

- **Every wasm1 object reads the error table packages.** With
  `MOON_SEPARATE_COMPILATION=1`, `wasm_object_inputs` appends the dependency
  closures of `moonbitlang/core/json` and `moonbitlang/core/debug`
  (`WASM_ERROR_TABLE_PACKAGES`) to the `.core` inputs of every object, and the
  executable links their objects. `moonc` compiles an error table entry, whose
  methods call into these packages, into each object using the error type, so
  standard library objects such as `builtin`, which do not depend on them,
  could not be compiled otherwise. This can go once the linker assembles each
  entry from the objects defining its methods.

If you need to add another compatibility shim, keep the behavior in the module
nearest to the code it affects, but document the motivation here so future work
can determine whether the special case is still necessary.
