// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::path::Path;

use moonutil::{compiler_flags::Toolchain, resolution::ModuleId};

use super::{
    BackendPlan, BuildCStubsInfo, BuildRuntimeInfo, BuildTargetInfo, CompileGeneratedCInfo,
    ForeignLibraryInfo, LinkCoreInfo, MakeExecutableInfo, PrebuildInfo,
};
use crate::model::{BuildPlanNode, BuildTarget, PackageId, TargetKind};

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
        /// The native toolchain of the action that consumes the linked core,
        /// if any. Its ABI selects the LLVM target.
        native_toolchain: Option<&'a Toolchain>,
    },
    MakeExecutable {
        target: BuildTarget,
        info: &'a MakeExecutableInfo,
    },
    CompileGeneratedC {
        target: BuildTarget,
        info: &'a CompileGeneratedCInfo,
    },
    ArchiveForeignLibrary {
        info: &'a ForeignLibraryInfo,
        runtime: &'a BuildRuntimeInfo,
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
}

impl BackendPlan {
    /// The native toolchain of the action that consumes `target`'s linked
    /// core: its executable, or, for a foreign library's source target, its
    /// archive. Both resolve the toolchain from the same package settings.
    fn linked_core_consumer_toolchain(&self, target: BuildTarget) -> Option<&Toolchain> {
        self.make_executable_info
            .get(&target)
            .map(|info| &info.effective_native_toolchain)
            .or_else(|| {
                (target.kind == TargetKind::Source)
                    .then(|| self.foreign_library_info.get(&target.package))
                    .flatten()
                    .map(|info| &info.effective_native_toolchain)
            })
    }

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
                native_toolchain: self.linked_core_consumer_toolchain(target),
            },
            BuildPlanNode::MakeExecutable(target) => BuildAction::MakeExecutable {
                target,
                info: self
                    .make_executable_info
                    .get(&target)
                    .expect("MakeExecutable nodes should contain native linking info"),
            },
            BuildPlanNode::CompileGeneratedC(target) => BuildAction::CompileGeneratedC {
                target,
                info: self
                    .compile_generated_c_info
                    .get(&target)
                    .expect("CompileGeneratedC nodes should contain native compile info"),
            },
            BuildPlanNode::ArchiveForeignLibrary(package) => BuildAction::ArchiveForeignLibrary {
                info: self
                    .foreign_library_info
                    .get(&package)
                    .expect("ArchiveForeignLibrary nodes should contain archive info"),
                runtime: self
                    .runtime_info
                    .as_ref()
                    .expect("Runtime info should be present for ArchiveForeignLibrary nodes"),
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

#[cfg(test)]
mod tests {
    use moonutil::compiler_flags::{ARKind, CC, CCKind, NativeAllocator};
    use slotmap::SlotMap;

    use super::*;
    use crate::build_plan::BuildPlan;

    #[test]
    fn foreign_library_link_core_uses_the_archive_toolchain() {
        let mut packages = SlotMap::<PackageId, ()>::with_key();
        let package = packages.insert(());
        let target = package.build_target(TargetKind::Source);
        let msvc = Toolchain::from_path_probe(CC {
            cc_kind: CCKind::Msvc,
            cc_path: "cl.exe".to_string(),
            ar_kind: ARKind::MsvcLib,
            ar_path: "lib.exe".to_string(),
            target_triple: None,
            is_env_override: false,
        });

        let mut plan = BuildPlan::default();
        plan.test_insert_link_core_info(
            target,
            LinkCoreInfo {
                linked_order: vec![target],
                link_stdlib_abort: false,
            },
        );
        plan.test_insert_foreign_library_info(
            package,
            ForeignLibraryInfo {
                effective_native_toolchain: msvc,
                c_stub_packages: Vec::new(),
                native_allocator: NativeAllocator::Default,
            },
        );

        // No executable consumes the linked core, so the LLVM target ABI must
        // come from the foreign library's archive toolchain.
        let BuildAction::LinkCore {
            native_toolchain, ..
        } = plan.backend_plan().action(BuildPlanNode::LinkCore(target))
        else {
            panic!("LinkCore nodes should hydrate to LinkCore actions");
        };
        assert!(native_toolchain.is_some_and(Toolchain::uses_msvc_abi));
    }
}
