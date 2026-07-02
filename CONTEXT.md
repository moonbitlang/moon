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

**Command Information Demand**:
The level of package information a MoonBuild command needs before it can produce its result.
_Avoid_: Always-full package model, eager package facts

**Lightweight Command**:
A MoonBuild command whose information demand stops before full dependency resolution and build target projection.
_Avoid_: Partial resolve, incomplete build command

## Naming

**Process Name**:
A verb or verb phrase used for operations that locate, read, scan, parse, resolve, project, or lower build information.
_Avoid_: Using process names for durable data entities

**Entity Name**:
A noun or noun phrase used for stable MoonBuild concepts produced or consumed by build stages. A distinct entity name does not require a distinct data container.
_Avoid_: Using entity names for operations, adding containers only to mirror vocabulary

## Native Build

**Native Payload Form**:
The representation produced by MoonBit before the host C compiler or linker is invoked, such as generated C or direct object code.
_Avoid_: Treating generated C, TCC execution, and direct object linking as the same kind of backend choice

**Native Command Driver**:
The concrete compiler/linker executable and archiver used by one native build action, such as `cl.exe`, `clang-cl.exe`, `cc`, or `tcc`.
_Avoid_: ABI family, process environment, build graph contract

**Native Build Contract**:
The single ABI family, CRT linkage, and toolchain environment selected for one native build invocation. Every native runtime build, C stub archive, generated-C compile/link, and direct-object link in that invocation must match this contract.
_Avoid_: Per-package ABI world, implicit compiler preference, duplicate shared native artifacts

**Native Toolchain**:
A native command driver paired with the native build contract it has been validated against. It is action-scoped; the durable graph-level choice is the native build contract.
_Avoid_: Raw compiler path, native backend mode, global per-package compiler

**Native Link Unit**:
The final native artifact together with every object file and archive that participates in one link. A native link unit consumes the invocation's native build contract.
_Avoid_: Per-package executable world, independent C command

**Native Toolchain Environment**:
The installation- and target-specific environment required to run a native toolchain, including the tool, header, library, and SDK search context.
_Avoid_: User shell, PATH lookup result, compiler flags

**ABI Family**:
The binary interface family fixed by the native toolchain. On Windows, MSVC and GNU-like toolchains are different ABI families and must not be mixed in one native executable.
_Avoid_: Compiler flavor, executable style

**CRT Linkage**:
The C runtime linkage policy required by an ABI family. For MSVC, every native object participating in one executable must use a consistent CRT linkage policy.
_Avoid_: Per-command compiler flag
