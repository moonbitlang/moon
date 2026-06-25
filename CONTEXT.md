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
