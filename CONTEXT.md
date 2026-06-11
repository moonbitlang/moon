# MoonBuild Context

MoonBuild is the build system and package manager implementation for MoonBit.
This context records project-specific terms that should stay stable across architecture discussions.

## Language

**Target backend**:
The user-visible platform selected for a build invocation, such as `wasm`, `wasm-gc`, `js`, `native`, or `llvm`.
_Avoid_: Backend when the distinction from execution strategy matters

**Native C toolchain**:
The resolved compiler driver, linker driver, archiver, command dialect, source of selection, and probed target facts used for native-oriented build steps.
_Avoid_: C compiler when the archiver, linker driver, or target facts matter

**Target platform**:
The target operating system, architecture, ABI, object format, and runtime ecosystem that native-oriented artifacts are built for.
_Avoid_: Host platform when cross-compilation is possible

**Native pipeline model**:
The internal model that decides how native-oriented artifacts move from link-core output to a runnable artifact: linked-core shape, runtime shape, C stub aggregation, final executable realization, and run invocation shape.
_Avoid_: Native backend, native mode, native executable strategy

**Backend lowering**:
The phase that turns the action-level build plan into concrete artifact paths, command lines, and an n2 build graph.
_Avoid_: Build execution, backend generation
