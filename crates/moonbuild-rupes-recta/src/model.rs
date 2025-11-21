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

use std::path::PathBuf;

use moonutil::{
    common::TargetBackend,
    mooncakes::{ModuleId, result::ResolvedEnv},
};

use crate::discover::DiscoverResult;

slotmap::new_key_type! {
    /// An unique identifier pointing to a package currently discovered from imported modules.
    pub struct PackageId;
}

/// Backend that affect how the build and artifact generation is performed.
///
/// Note: This is different from [`TargetBackend`]. That enum is a high-level
/// abstraction of the user's choice and what kind of output format `moonc`
/// produces, but this also cares about what toolchains are used, etc.
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum RunBackend {
    WasmGC,
    Wasm,
    Js,
    Native,
    /// Like `Native`, but uses `tcc -run` to execute the program directly. Does
    /// not produce a standalone binary artifact.
    NativeTccRun,
    Llvm,
}

impl RunBackend {
    pub fn is_native(self) -> bool {
        matches!(
            self,
            RunBackend::Native | RunBackend::NativeTccRun | RunBackend::Llvm
        )
    }

    pub fn to_target(self) -> TargetBackend {
        self.into()
    }
}

impl From<RunBackend> for TargetBackend {
    fn from(val: RunBackend) -> Self {
        match val {
            RunBackend::WasmGC => TargetBackend::WasmGC,
            RunBackend::Wasm => TargetBackend::Wasm,
            RunBackend::Js => TargetBackend::Js,
            RunBackend::Native => TargetBackend::Native,
            RunBackend::NativeTccRun => TargetBackend::Native,
            RunBackend::Llvm => TargetBackend::LLVM,
        }
    }
}

/// Represents the overall action of this build tool call
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum RunAction {
    Build,
    Bundle,
    Check,
    Test,
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

impl TargetKind {
    pub fn is_test(self) -> bool {
        matches!(
            self,
            TargetKind::WhiteboxTest | TargetKind::BlackboxTest | TargetKind::InlineTest
        )
    }

    pub fn all_tests() -> &'static [TargetKind] {
        &[
            TargetKind::WhiteboxTest,
            TargetKind::BlackboxTest,
            TargetKind::InlineTest,
        ]
    }
}

/// Represents a single compile target that may be separately checked, built,
/// linked, etc.
#[derive(Clone, PartialEq, Eq, Hash, Copy, PartialOrd, Ord)]
pub struct BuildTarget {
    pub package: PackageId,
    pub kind: TargetKind,
    // TODO: Target backend need to be added here!
}

impl std::fmt::Debug for BuildTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}@{:?}", self.package, self.kind)
    }
}

impl PackageId {
    pub fn build_target(self, kind: TargetKind) -> BuildTarget {
        BuildTarget {
            package: self,
            kind,
        }
    }
}

/// A node in the build dependency graph, containing a build target and the
/// corresponding action that should be performed on that target.
///
/// TODO: This type is a little big in size to be copied and used as an ID.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum BuildPlanNode {
    Check(BuildTarget),
    BuildCore(BuildTarget),
    /// Build the i-th C file in the C stub list.
    BuildCStub(PackageId, u32), // change into global artifact list if we need non-package ones
    ArchiveOrLinkCStubs(PackageId),
    LinkCore(BuildTarget),
    MakeExecutable(BuildTarget),
    GenerateTestInfo(BuildTarget),
    GenerateMbti(BuildTarget),
    Bundle(ModuleId),
    BuildRuntimeLib,

    /// Build the virtual package's `.mbti` interface file to get an `.mi` file.
    BuildVirtual(PackageId),

    /// Run the i-th prebuild script in the prebuild script list.
    RunPrebuild(PackageId, u32),

    /// Run the i-th prebuild rule for `moonlex` predefined prebuild.
    RunMoonLexPrebuild(PackageId, u32),
    /// Run the i-th prebuild rule for `moonyacc` predefined prebuild.
    RunMoonYaccPrebuild(PackageId, u32),

    /// Docs build is currently for everything.
    ///
    /// The legacy layout does not have a separate folder for different kinds
    /// of docs, and the behavior is dictated by `packages.json`, so we can't
    /// do much better for now.
    BuildDocs,
}

impl BuildPlanNode {
    pub fn check(target: BuildTarget) -> Self {
        Self::Check(target)
    }

    pub fn build_core(target: BuildTarget) -> Self {
        Self::BuildCore(target)
    }

    pub fn link_core(target: BuildTarget) -> Self {
        Self::LinkCore(target)
    }

    pub fn make_executable(target: BuildTarget) -> Self {
        Self::MakeExecutable(target)
    }

    pub fn generate_test_info(target: BuildTarget) -> Self {
        Self::GenerateTestInfo(target)
    }

    /// Extract the target from a BuildPlanNode, if it has one
    pub fn extract_target(&self) -> Option<BuildTarget> {
        match *self {
            BuildPlanNode::Check(target)
            | BuildPlanNode::BuildCore(target)
            | BuildPlanNode::LinkCore(target)
            | BuildPlanNode::MakeExecutable(target)
            | BuildPlanNode::GenerateTestInfo(target)
            | BuildPlanNode::GenerateMbti(target) => Some(target),
            BuildPlanNode::BuildCStub(_, _)
            | BuildPlanNode::ArchiveOrLinkCStubs(_)
            | BuildPlanNode::Bundle(_)
            | BuildPlanNode::BuildRuntimeLib
            | BuildPlanNode::BuildDocs
            | BuildPlanNode::BuildVirtual(_)
            | BuildPlanNode::RunPrebuild(_, _)
            | BuildPlanNode::RunMoonLexPrebuild(_, _)
            | BuildPlanNode::RunMoonYaccPrebuild(_, _) => None,
        }
    }

    /// Return a concise, human-readable identifier resolving PackageId/ModuleId to names.
    /// Single-line and stable; suitable for filenames/labels (e.g. n2 fileloc).
    pub fn string_id(&self, env: &ResolvedEnv, packages: &DiscoverResult) -> String {
        match self {
            BuildPlanNode::Check(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@Check", fqn, t.kind)
            }
            BuildPlanNode::BuildCore(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@BuildCore", fqn, t.kind)
            }
            BuildPlanNode::BuildCStub(pkg, idx) => {
                let fqn = packages.fqn(*pkg);
                format!("{}@BuildCStub_{}", fqn, idx)
            }
            BuildPlanNode::ArchiveOrLinkCStubs(pkg) => {
                let fqn = packages.fqn(*pkg);
                format!("{}@ArchiveCStubs", fqn)
            }
            BuildPlanNode::LinkCore(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@LinkCore", fqn, t.kind)
            }
            BuildPlanNode::MakeExecutable(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@MakeExecutable", fqn, t.kind)
            }
            BuildPlanNode::GenerateTestInfo(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@GenerateTestInfo", fqn, t.kind)
            }
            BuildPlanNode::GenerateMbti(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@GenerateMbti", fqn, t.kind)
            }
            BuildPlanNode::Bundle(mid) => {
                let src = env.mod_name_from_id(*mid);
                format!("{}@Bundle", src)
            }
            BuildPlanNode::BuildRuntimeLib => "BuildRuntimeLib".to_string(),
            BuildPlanNode::BuildVirtual(pkg) => {
                let fqn = packages.fqn(*pkg);
                format!("{}@BuildVirtual", fqn)
            }
            BuildPlanNode::RunPrebuild(pkg, idx) => {
                let fqn = packages.fqn(*pkg);
                format!("{}@RunPrebuild_{}", fqn, idx)
            }
            BuildPlanNode::RunMoonLexPrebuild(pkg, idx) => {
                let fqn = packages.fqn(*pkg);
                format!("{}@RunMoonLexPrebuild_{}", fqn, idx)
            }
            BuildPlanNode::RunMoonYaccPrebuild(pkg, idx) => {
                let fqn = packages.fqn(*pkg);
                format!("{}@RunMoonYaccPrebuild_{}", fqn, idx)
            }
            BuildPlanNode::BuildDocs => "BuildDocs".to_string(),
        }
    }
}

/// Represents a list of artifact(s) corresponding to a single build node.
#[derive(Clone, Debug)]
pub struct Artifacts {
    pub node: BuildPlanNode,
    pub artifacts: Vec<PathBuf>,
}

/// Supported operating systems for artifact generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatingSystem {
    Windows,
    Linux,
    MacOS,
    /// No operating system (e.g., WASM/JS targets)
    None,
}

impl std::fmt::Display for OperatingSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            OperatingSystem::Windows => "windows",
            OperatingSystem::Linux => "linux",
            OperatingSystem::MacOS => "macos",
            OperatingSystem::None => "none",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for OperatingSystem {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "windows" => Ok(OperatingSystem::Windows),
            "linux" => Ok(OperatingSystem::Linux),
            "macos" => Ok(OperatingSystem::MacOS),
            "none" => Ok(OperatingSystem::None),
            _ => Err(format!("Unsupported OS: {}", s)),
        }
    }
}
