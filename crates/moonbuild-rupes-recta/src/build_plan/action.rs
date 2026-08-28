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

use std::path::Path;

use moonutil::resolution::ModuleId;

use super::{
    BuildCStubsInfo, BuildRuntimeInfo, BuildTargetInfo, LinkCoreInfo, MakeExecutableInfo,
    PrebuildInfo,
};
use crate::model::{BuildTarget, PackageId};

/// A semantic Build Plan action hydrated with the metadata needed by lowering.
///
/// This borrowed value is constructed on demand; it is not a second stored
/// action graph.
#[derive(Clone, Copy, Debug)]
pub(crate) enum BuildAction<'a> {
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
        info: &'a MakeExecutableInfo,
    },
    GenerateDsym {
        target: BuildTarget,
        dsymutil: &'a Path,
    },
    GenerateTestInfo {
        target: BuildTarget,
        info: &'a BuildTargetInfo,
    },
    GenerateNodeTestPackageConfig {
        package: PackageId,
    },
    GenerateMbti {
        target: BuildTarget,
    },
    BuildVirtual {
        package: PackageId,
        input: &'a Path,
    },
    Bundle {
        module: ModuleId,
        targets: &'a [BuildTarget],
    },
    BuildRuntimeObject {
        index: u32,
        info: &'a BuildRuntimeInfo,
    },
    BuildRuntimeLib {
        info: &'a BuildRuntimeInfo,
    },
    BuildDocs {
        module: ModuleId,
    },
    RunPrebuild {
        info: &'a PrebuildInfo,
    },
    RunMoonLexPrebuild {
        package: PackageId,
        input: &'a Path,
        output: &'a Path,
    },
    RunMoonYaccPrebuild {
        package: PackageId,
        input: &'a Path,
        output: &'a Path,
    },
}
