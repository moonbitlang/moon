// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

/*
    *************
    MAINTAINERS: Please update this BEFORE you add new features, in order
    to keep this section synced with source code.
    *************
*/
/*!
    Refactor of the task generation process.

    # Overview of task generation

    Different tasks of the building process of a package (compile unit) rely on
    different source files and parent tasks. Here's a layered view of it:

    There are 4 kinds of source files in the within each package:
    - Source. These are the default kind of MoonBit files.
    - C stubs. These are the C files that reside beside MoonBit files.
    - Whitebox tests.
    - Blackbox tests.

    Within a single package, there are 3 main targets:
    - Source (containing source files Source and C stubs)
    - Whitebox test (containing source file Whitebox tests)
    - Blackbox test (containing source file Blackbox tests)

    For tests, there are also doctests and markdown tests, but that's mostly
    the same as Blackbox tests.

    Each target has the following dependency between tasks:
    - Check: (check of direct dependencies)
    - Build: (build of direct dependencies)
    - Build-C-stubs: (none)
    - Link-core: Build, (build of all direct and indirect dependencies)
    - Make-executable: Link-core, Build-C-stubs (if any)
    - Generate-MBTI: (either Check or Build)

    And both Whitebox test and Blackbox test additionally have an implicit
    direct dependency on Source.

    From external view, there are 4 ultimate tasks of the project:
    - Check, corresponding to all Check tasks of all targets.
    - Build, corresponding to all Make-executable tasks of Source
    - Bundle, corresponding to all Build tasks of non-main sources, and in
      addition a commandline call to `moon bundle`.
    - Test, corresponding to all Make-executable tasks of Whitebox tests and
      Blackbox tests. After all tasks are built, we run all executables for
      test.
    - Generate MBTI, corresponding to all Generate-MBTI tasks of Source.
*/

use std::{collections::HashMap, path::PathBuf};

use arcstr::ArcStr;
use moonutil::mooncakes::ModuleId;
use petgraph::prelude::DiGraphMap;

slotmap::new_key_type! {
    /// An unique identifier pointing to a package currently discovered from imported modules.
    pub struct PackageId;
}

/// Represents the target of this build routine.
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum RunTask {
    Build,
    Bundle,
    Check,
    Test,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum TargetTask {
    Check,
    Build,
    BuildCStubs,
    LinkCore,
    MakeExecutable,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum TargetKind {
    Source,
    WhiteboxTest,
    BlackboxTest,

    // TODO: do we really need to specify inline tests as a separate target kind,
    // or should it be just `Source` with tests enabled?
    InlineTest,
    /// This is the subpackage designed originally for breaking cycles in
    /// `moonbitlang/core`. It's expected to be used sparingly.
    SubPackage,
}

/// Represents a single compile target that may be separately checked, built,
/// linked, etc.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Copy, PartialOrd, Ord)]
pub struct BuildTarget {
    pub package: PackageId,
    pub kind: TargetKind,
}

impl PackageId {
    pub fn build_target(self, kind: TargetKind) -> BuildTarget {
        BuildTarget {
            package: self,
            kind,
        }
    }
}

/// Represents a single target, like ordinary source, whitebox test files, etc.
/// This is a smaller unit than `Package`, and is the actual compile unit.
#[derive(Clone, Debug, Default)]
pub struct Target {
    files: Vec<PathBuf>,
    c_stubs: Vec<PathBuf>,
    /// The dependent package names
    deps: Vec<BuildTarget>,
}

#[derive(Clone, Debug, Default)]
pub struct PackageTargets {
    source_target: Target,
    whitebox_test: Option<Target>,
    blackbox_test: Option<Target>,
}
