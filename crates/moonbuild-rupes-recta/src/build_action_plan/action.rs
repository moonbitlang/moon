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

use moonutil::mooncakes::ModuleId;

use crate::{
    build_plan::{
        BuildCStubsInfo, BuildTargetInfo, LinkCoreInfo, MakeExecutableInfo, PrebuildInfo,
    },
    model::{BuildTarget, PackageId},
};

/// Opaque identifier for an action in a [`BuildActionPlan`](super::BuildActionPlan).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct BuildActionId(pub(crate) usize);

/// A build action with all planning metadata needed by backend lowering.
#[derive(Clone, Copy, Debug)]
pub enum BuildAction<'a> {
    Check {
        target: BuildTarget,
        info: &'a BuildTargetInfo,
    },
    EmitProof {
        target: BuildTarget,
        info: &'a BuildTargetInfo,
    },
    Prove {
        target: BuildTarget,
        info: &'a BuildTargetInfo,
    },
    BuildCore {
        target: BuildTarget,
        info: &'a BuildTargetInfo,
    },
    BuildCStub {
        package: PackageId,
        index: u32,
        info: &'a BuildCStubsInfo,
    },
    ArchiveOrLinkCStubs {
        package: PackageId,
        info: &'a BuildCStubsInfo,
    },
    LinkCore {
        target: BuildTarget,
        info: &'a LinkCoreInfo,
        make_executable_info: Option<&'a MakeExecutableInfo>,
    },
    MakeExecutable {
        target: BuildTarget,
        info: Option<&'a MakeExecutableInfo>,
    },
    GenerateTestInfo {
        target: BuildTarget,
        info: &'a BuildTargetInfo,
    },
    GenerateMbti {
        target: BuildTarget,
    },
    BuildVirtual {
        package: PackageId,
    },
    Bundle {
        module: ModuleId,
        targets: &'a [BuildTarget],
    },
    BuildRuntimeLib,
    BuildDocs {
        module: ModuleId,
    },
    RunPrebuild {
        package: PackageId,
        index: u32,
        info: &'a PrebuildInfo,
    },
    RunMoonLexPrebuild {
        package: PackageId,
        index: u32,
    },
    RunMoonYaccPrebuild {
        package: PackageId,
        index: u32,
    },
}
