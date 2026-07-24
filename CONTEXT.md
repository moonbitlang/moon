# MoonBuild

MoonBuild turns MoonBit project declarations and package files into build targets, dependency relationships, and executable build commands.

## Language

**Package Declaration**:
The declaration-level representation of a package: its identity, root, manifest configuration, and package-level declared capabilities.
_Avoid_: Discovered package, package discovery result, package facts

**Package File Set**:
The set of MoonBuild-relevant file paths observed under an already known package root before target-specific selection.
_Avoid_: Package file inventory, package file facts, source file discovery, package source files

**File Content**:
The bytes or text read from a file after it has been identified as relevant.
_Avoid_: File facts

**File Interpretation**:
The meaning MoonBuild derives from a file path or file content before applying target-specific projection.
_Avoid_: File facts, parsed file

**Build Rule**:
A declared or built-in rule that constrains how declarations, file sets, and file interpretations participate in a build.
_Avoid_: Build fact

**Build Target Projection**:
The target-specific view produced by applying build rules to package declarations, package file sets, and file interpretations. It is not the source of truth for package content.
_Avoid_: Build target facts, target file facts

**Test Driver Event**:
A record emitted by a generated test driver that describes a lifecycle or result fact for one selected test case.
_Avoid_: Treating every driver record as a test result

**Command Information Demand**:
The level of package information a MoonBuild command needs before it can produce its result.
_Avoid_: Always-full package model, eager package facts

**Lightweight Command**:
A MoonBuild command whose information demand stops before full dependency resolution and build target projection.
_Avoid_: Partial resolve, incomplete build command

## Command Communication

**Command Result**:
The stdout representation a MoonBuild command intentionally produces for a user or another program. It is not filtered by `--quiet` or `--verbose`.
_Avoid_: User log, progress message, child output

**User Log**:
A MoonBuild-authored stderr message that explains an error, warning, informational event, or debugging detail. Its visibility is controlled by the command's user-log level.
_Avoid_: Command result, tracing log, child output

**Process Passthrough**:
The stdout or stderr bytes of a child tool or executed program forwarded without MoonBuild changing their channel, content, or visibility.
_Avoid_: Command result, user log

**Progress Display**:
Transient terminal status for concurrent or long-running work, rendered independently from durable user-log lines.
_Avoid_: User log, command result

## Package Execution

**Executable Package Coordinate**:
An exact registry selector containing a module, an optional package path, and an optional version that identifies one main package for direct execution.
_Avoid_: Install source, wildcard package selector, runwasm coordinate

**Cached Executable Artifact**:
The final runnable file retained for an Executable Package Coordinate and Target Backend so later executions can skip acquisition or building.
_Avoid_: Installed binary, cached build workspace, source cache

**Prepared Dependency Build Graph**:
The complete set of validated compiler products for the resolved registry dependencies of one standalone script compilation. It is immutable after publication and excludes the script package, `packages.json`, and mutable n2 workspace.
_Avoid_: Shared build directory, global `packages.json`, Cached Executable Artifact

## Naming

**Process Name**:
A verb or verb phrase used for operations that locate, read, scan, parse, resolve, project, or lower build information.
_Avoid_: Using process names for durable data entities

**Entity Name**:
A noun or noun phrase used for stable MoonBuild concepts produced or consumed by build stages. A distinct entity name does not require a distinct data container.
_Avoid_: Using entity names for operations, adding containers only to mirror vocabulary

## Native Build

**Target Backend**:
The user-visible backend selection for a build, such as Native, LLVM, Wasm, WasmGC, or JS.
_Avoid_: Native target

**Generated-C Native Backend**:
The Native target backend implementation where `moonc link-core` emits C and MoonBuild invokes a C toolchain to compile and link it.
_Avoid_: Native target, direct object target

**Direct Object Native Target**:
The concrete architecture/OS/ABI target used by the experimental direct object-code native path, such as `x86_64-pc-windows-msvc`.
_Avoid_: Generated-C native backend, native backend

**Native Payload Form**:
The representation produced by MoonBit before the host C compiler or linker is invoked, such as generated C or direct object code.
_Avoid_: Treating generated C, TCC execution, and direct object linking as the same kind of backend choice

**Native Toolchain**:
The selected native compiler/linker used after MoonBit lowering, together with any ABI family, command environment, and runtime-linkage obligations it imposes on runtime, C stubs, and executable linking.
_Avoid_: Raw compiler path, native backend mode

**ABI Family**:
The binary interface family fixed by the native toolchain. On Windows, MSVC and GNU-like toolchains are different ABI families and must not be mixed in one native executable.
_Avoid_: Compiler flavor, executable style

**CRT Linkage**:
The C runtime linkage policy required by an ABI family. For MSVC, every native object participating in one executable must use a consistent CRT linkage policy.
_Avoid_: Per-command compiler flag
