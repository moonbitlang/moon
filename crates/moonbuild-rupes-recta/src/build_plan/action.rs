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
    BackendPlan, BuildCStubsInfo, BuildRuntimeInfo, BuildTargetInfo, LinkCoreInfo,
    MakeExecutableInfo, PrebuildInfo,
};
use crate::model::{BuildPlanNode, BuildTarget, PackageId};

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

impl BackendPlan {
    /// Hydrate a backend action from its owning plan's metadata.
    pub(crate) fn action(&self, node: BuildPlanNode) -> BuildAction<'_> {
        match node {
            BuildPlanNode::Check(target) => BuildAction::Check {
                target,
                info: self
                    .build_target_infos
                    .get(&target)
                    .expect("Build target info should be present for Check nodes"),
            },
            BuildPlanNode::EmitProof(target) => BuildAction::EmitProof {
                target,
                info: self
                    .build_target_infos
                    .get(&target)
                    .expect("Build target info should be present for EmitProof nodes"),
            },
            BuildPlanNode::Prove(target) => BuildAction::Prove {
                target,
                info: self
                    .build_target_infos
                    .get(&target)
                    .expect("Build target info should be present for Prove nodes"),
            },
            BuildPlanNode::BuildCore(target) => BuildAction::BuildCore {
                target,
                info: self
                    .build_target_infos
                    .get(&target)
                    .expect("Build target info should be present for BuildCore nodes"),
            },
            BuildPlanNode::BuildCStub(package, index) => BuildAction::BuildCStub {
                package,
                index,
                info: self
                    .c_stubs_info
                    .get(&package)
                    .expect("C stub info should be present for BuildCStub nodes"),
            },
            BuildPlanNode::ArchiveOrLinkCStubs(package) => BuildAction::ArchiveOrLinkCStubs {
                package,
                info: self
                    .c_stubs_info
                    .get(&package)
                    .expect("C stubs info should be present for BuildCStubs nodes"),
            },
            BuildPlanNode::LinkCore(target) => BuildAction::LinkCore {
                target,
                info: self
                    .link_core_info
                    .get(&target)
                    .expect("Link core info should be present for LinkCore nodes"),
                make_executable_info: self.make_executable_info.get(&target),
            },
            BuildPlanNode::MakeExecutable(target) => BuildAction::MakeExecutable {
                target,
                info: self
                    .make_executable_info
                    .get(&target)
                    .expect("MakeExecutable nodes should contain native linking info"),
            },
            BuildPlanNode::GenerateDsym(target) => BuildAction::GenerateDsym {
                target,
                dsymutil: self
                    .dsymutil
                    .as_deref()
                    .expect("dsymutil should be present for GenerateDsym nodes"),
            },
            BuildPlanNode::GenerateTestInfo(target) => BuildAction::GenerateTestInfo {
                target,
                info: self
                    .build_target_infos
                    .get(&target)
                    .expect("Build target info should be present for GenerateTestInfo nodes"),
            },
            BuildPlanNode::GenerateNodeTestPackageConfig(package) => {
                BuildAction::GenerateNodeTestPackageConfig { package }
            }
            BuildPlanNode::GenerateMbti(target) => BuildAction::GenerateMbti { target },
            BuildPlanNode::BuildVirtual(package) => BuildAction::BuildVirtual {
                package,
                input: self
                    .virtual_contract_inputs
                    .get(&package)
                    .map(|path| path.as_path())
                    .expect("virtual contract input should be selected during build planning"),
            },
            BuildPlanNode::Bundle(module) => BuildAction::Bundle {
                module,
                targets: &self
                    .bundle_info
                    .get(&module)
                    .expect("Bundle info should be present when lowering bundle node")
                    .bundle_targets,
            },
            BuildPlanNode::BuildRuntimeObject(index) => BuildAction::BuildRuntimeObject {
                index,
                info: self
                    .runtime_info
                    .as_ref()
                    .expect("Runtime info should be present for runtime object nodes"),
            },
            BuildPlanNode::BuildRuntimeLib => BuildAction::BuildRuntimeLib {
                info: self
                    .runtime_info
                    .as_ref()
                    .expect("Runtime info should be present for BuildRuntimeLib nodes"),
            },
            BuildPlanNode::BuildDocs(module) => BuildAction::BuildDocs { module },
        }
    }
}
