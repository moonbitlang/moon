# TCC Run Mode (Fast CC) — How it differs from regular builds

This document explains the TCC run mode (“fast CC”) across build and test, how it is modelled in the build graph, and how it differs from regular native builds. It describes the current behavior implemented in Moon’s generator and CLI.

## What “TCC run” is

- Codegen stays on the Native backend: we still produce a C artifact via `moonc link-core`.
- Execution differs: instead of compiling that C artifact into a native executable and running it, we invoke the internal `tcc` with `-run` to execute the C directly.
- Artifact shapes differ under TCC: runtime is built as a shared library, and C stubs are linked to dynamic libraries.

Key references:

- Link-core output and exported functions: [gen_runtest::gen_runtest_link_command()](crates/moonbuild/src/gen/gen_runtest.rs:1235)
- Example tcc-run command shape in CLI run: [run.rs](crates/moon/src/cli/run.rs:193)
- Internal TCC detection: [CC::internal_tcc()](crates/moonutil/src/compiler_flags.rs:208)

## When TCC run applies

TCC run is enabled only when all gates pass:

- Native backend (not JS/WASM/LLVM)
- Non-Windows (runtime.c cannot be built with tcc on Windows)
- Debug builds (fast iteration) and in test/run modes that opt into fast CC
- Package does not set custom `cc`, `cc-flags`, or `cc-link-flags` (user control takes precedence)

References for gating:

- CLI run gating: [run.rs](crates/moon/src/cli/run.rs:188)
- CLI test gating and warnings: [test.rs](crates/moon/src/cli/test.rs:1325) and [test.rs](crates/moon/src/cli/test.rs:1334)

## Build graph changes (legacy generator behaviour)

The legacy generator already branches on `use_tcc_run` and changes artifacts accordingly:

- Runtime artifact:

  - TCC: build shared runtime and add to default targets: [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1386)
  - Non-TCC: build a plain `runtime.o`: [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1390)

- Stub libraries:

  - Non-TCC: archive stub objects into static libraries `.a`: [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1445)
  - TCC: link stubs into dynamic libraries `.so`/`.dylib` and add them to default: [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1451)

- Executable step:

  - Non-TCC Native: compile C to a native executable: [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1399)
  - LLVM: perform a separate link to an executable: [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1411)
  - TCC Native: skip compiling to exe; run via `tcc -run` using the shared runtime and dynamic stubs

- Link-core (unchanged): always produces the C output and exports test driver entry points: [gen_runtest::gen_runtest_link_command()](crates/moonbuild/src/gen/gen_runtest.rs:1235)

## Compiler and linker configuration differences under TCC

TCC has different toolchain behaviour; the build driver must render flags accordingly. Use `CC.is_tcc()` to gate toolchain-specific quirks:

- No separate `ar`; use `tcc -ar`: [compiler_flags.rs](crates/moonutil/src/compiler_flags.rs:362)
- Include/library flags use tcc forms, not MSVC or clang forms:
  - `-I<path>`, `-L<path>`: [compiler_flags.rs](crates/moonutil/src/compiler_flags.rs:644) and [compiler_flags.rs](crates/moonutil/src/compiler_flags.rs:457)
- Define `MOONBIT_NATIVE_NO_SYS_HEADER` to avoid problematic system header behaviour in tcc:
  - [compiler_flags.rs](crates/moonutil/src/compiler_flags.rs:302)
- Do not link or archive `libmoonbitrun.o` under tcc; only warn if asked:
  - Archive warning: [compiler_flags.rs](crates/moonutil/src/compiler_flags.rs:381)
  - Link warning: [compiler_flags.rs](crates/moonutil/src/compiler_flags.rs:505)

At a higher level, compile/link configs flip under TCC:

- Do not link `libmoonbitrun`: [gen_build.rs](crates/moonbuild/src/gen/gen_build.rs:979)
- Define shared-runtime macro so generated code resolves runtime APIs from the shared library: [gen_build.rs](crates/moonbuild/src/gen/gen_build.rs:981)

## Running under TCC (current behavior)

The runner invokes the internal tcc with `-run`, passing Moon’s include and lib paths, the runtime source, and defining `MOONBIT_NATIVE_NO_SYS_HEADER`. The linked C produced by `moonc link-core` is the program body.

Schematic (shape from CLI):
`tcc -I <moon include> -L <moon lib> <moon lib>/runtime.c -lm -DMOONBIT_NATIVE_NO_SYS_HEADER -run <output.c>`
See [run.rs](crates/moon/src/cli/run.rs:193).

## Backend semantics

TCC run uses the Native backend for codegen; `moonc link-core` still targets Native and produces C (with exported test driver entry points). The differences are in runtime/stub artifacts and in execution (using `tcc -run`).

## Target directory and default targets

Under TCC:

- Shared runtime `.so`/`.dylib` should be explicitly included in default targets (not referenced by any single target otherwise): [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1386)
- Dynamic stub libraries also get added to defaults so they are built before the run: [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1451)

Under non-TCC:

- Stubs are archived `.a` and pulled in via native link; no need to be default outputs: [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1445)

## Coverage notes

- Build-package coverage is enabled only when not blackbox and not third-party: [gen_runtest::enable_coverage_during_compile()](crates/moonbuild/src/gen/gen_runtest.rs:1597)
- Test driver generation passes coverage collection flags independently of blackbox coverage behaviour: [gen_runtest::gen_generate_test_driver_command()](crates/moonbuild/src/gen/gen_runtest.rs:1495)

## Virtual packages

TCC run does not change virtual package semantics:

- Link traversal and substitution of implementation cores continue to happen the same way.
- See substitution: [gen_build::replace_virtual_pkg_core_with_impl_pkg_core()](crates/moonbuild/src/gen/gen_build.rs:769)

## End-to-end flow (summary)

```mermaid
flowchart TD
  subgraph Common
    A[moonc build-package -> .core]
    B[moonc link-core -> .c (exports test driver entry points)]
    A --> B
  end

  subgraph Non-TCC (Native/LLVM)
    B --> C[Compile/Link Executable]
    subgraph Stubs
      S1[Compile stub .o] --> S2[Archive static .a]
    end
    subgraph Runtime
      R1[Compile runtime.o]
    end
    C --> D[Run exe]
  end

  subgraph TCC Run (Native fast CC)
    B --> RT[Build shared runtime (.so/.dylib)]
    B --> SD[Build stub dynamic libs (.so/.dylib)]
    RT --> TR[tcc -I/-L runtime.c -lm -DMOONBIT_NATIVE_NO_SYS_HEADER -run <.c>]
    SD --> TR
  end
```

## Practical implications

- TCC run uses Native backend for codegen; `moonc link-core` emits C and exported test functions.
- artifact kinds differ from regular native runs: shared runtime and dynamic stub libraries are built; the native exe compile step is skipped.
- Behavior is gated by platform/backend/mode and by per-package CC override checks; when overrides exist, the run falls back to the regular native executable.
- Respect platform constraints and user overrides; when gates fail, tests run via the regular executable.

## Cross-references (selected)

- Link-core command for tests (exports): [gen_runtest::gen_runtest_link_command()](crates/moonbuild/src/gen/gen_runtest.rs:1235)
- Runtime shared vs object: [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1386) / [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1390)
- Stub archive vs dynamic link: [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1445) / [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1451)
- Skip native exe under TCC; compile/link exe otherwise: [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1399) / [gen_runtest.rs](crates/moonbuild/src/gen/gen_runtest.rs:1411)
- CLI tcc run shape: [run.rs](crates/moon/src/cli/run.rs:193)
- TCC toolchain helpers and flags: [compiler_flags.rs](crates/moonutil/src/compiler_flags.rs:196), [compiler_flags.rs](crates/moonutil/src/compiler_flags.rs:362), [compiler_flags.rs](crates/moonutil/src/compiler_flags.rs:644), [compiler_flags.rs](crates/moonutil/src/compiler_flags.rs:302), [compiler_flags.rs](crates/moonutil/src/compiler_flags.rs:381), [compiler_flags.rs](crates/moonutil/src/compiler_flags.rs:505)
- Compile/link config flips (shared runtime macro, no moonbitrun): [gen_build.rs](crates/moonbuild/src/gen/gen_build.rs:979) and [gen_build.rs](crates/moonbuild/src/gen/gen_build.rs:981)

## Test runner detection and custom CC overrides

How the test runner decides TCC vs regular executable:

- The decision is made up front per invocation. Default enabling rule for tests is: debug build AND run mode is test. See [crates/moon/src/cli/test.rs](crates/moon/src/cli/test.rs:1325).
- Then for every package in the module (after filter), Moon checks if a package has any native CC overrides (cc, cc-flags, cc-link-flags). If any such override exists, TCC run is globally disabled for the whole test invocation, and a warning is emitted naming the offending package. See [crates/moon/src/cli/test.rs](crates/moon/src/cli/test.rs:1334).
  - The specific logic is: `use_tcc_run &= n.cc.is_none() && n.cc_flags.is_none() && n.cc_link_flags.is_none();` and compare `old_flag` to emit a warning when flipping. See [crates/moon/src/cli/test.rs](crates/moon/src/cli/test.rs:1336).
- Platform gating applies before this: TCC-run is disabled on Windows and only considered for Native backend debug runs. See [crates/moon/src/cli/run.rs](crates/moon/src/cli/run.rs:188).

Behavior when a package sets custom CC in tests:

- TCC run is disabled for the entire test run, even if only one package has overrides. This avoids mixing toolchains in a single run (which would cause inconsistent link behavior and hard-to-debug runtime differences).
- The runner falls back to the regular native executable pipeline:
  - Link-core still produces the C output: [gen_runtest_link_command()](crates/moonbuild/src/gen/gen_runtest.rs:1235)
  - Runtime built as `runtime.o` instead of shared: [gen_n2_runtest_state()](crates/moonbuild/src/gen/gen_runtest.rs:1390)
  - Stubs archived into static `.a` instead of dynamic `.so/.dylib`: [gen_n2_runtest_state()](crates/moonbuild/src/gen/gen_runtest.rs:1445)
  - Native exe is compiled and executed: [gen_n2_runtest_state()](crates/moonbuild/src/gen/gen_runtest.rs:1399)
- Coverage/test-driver behavior remains the same with respect to mode-specific coverage gating (e.g., blackbox compile coverage disabled): [enable_coverage_during_compile()](crates/moonbuild/src/gen/gen_runtest.rs:1597) and generation of driver metadata: [gen_generate_test_driver_command()](crates/moonbuild/src/gen/gen_runtest.rs:1495).

Summary:

- Detect TCC-run using platform/backend/mode conditions and per-package CC overrides; if any overrides exist, disable TCC-run globally and warn.
- Under TCC-run, build shared runtime and dynamic stubs and run `tcc -run` on the linked C artifact.
- Under regular run, build `runtime.o`, archive static stubs, compile a native executable, and run it.
