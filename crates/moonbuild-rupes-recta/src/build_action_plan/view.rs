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

use std::collections::{HashMap, HashSet};

use moonutil::resolution::ResolvedEnv;

use crate::{
    build_plan::{ArtifactKey, BuildPlan, FileDependencyKind},
    discover::DiscoverResult,
    model::{BuildPlanNode, BuildTarget, PackageId},
};

use super::{BuildAction, BuildActionId, BuildProduct};

/// Normalized action-level view consumed by backend lowering.
pub struct BuildActionPlan<'a> {
    plan: &'a BuildPlan,
    action_nodes: Vec<BuildPlanNode>,
    action_ids_by_node: HashMap<BuildPlanNode, BuildActionId>,
    input_actions: Vec<BuildActionId>,
}

impl BuildPlan {
    pub fn build_action_plan(&self) -> BuildActionPlan<'_> {
        let action_nodes = self.all_nodes().collect::<Vec<_>>();
        let action_ids_by_node = action_nodes
            .iter()
            .copied()
            .enumerate()
            .map(|(idx, node)| (node, BuildActionId(idx)))
            .collect::<HashMap<_, _>>();
        let input_actions = self
            .input_nodes()
            .iter()
            .map(|node| {
                *action_ids_by_node
                    .get(node)
                    .expect("input node should be present in build action plan")
            })
            .collect();
        BuildActionPlan {
            plan: self,
            action_nodes,
            action_ids_by_node,
            input_actions,
        }
    }
}

impl<'a> BuildActionPlan<'a> {
    pub fn action_ids(&self) -> impl Iterator<Item = BuildActionId> + '_ {
        (0..self.action_nodes.len()).map(BuildActionId)
    }

    pub fn input_action_ids(&self) -> &[BuildActionId] {
        &self.input_actions
    }

    /// Separate reusable package preparation from work owned by the
    /// synthesized script package while preserving the original action edges.
    pub(crate) fn partition_standalone_actions(
        &self,
        script_package: PackageId,
    ) -> (Vec<BuildActionId>, Vec<BuildActionId>) {
        let action_package = |action| match action {
            BuildAction::Check { target, .. }
            | BuildAction::EmitProof { target, .. }
            | BuildAction::Prove { target, .. }
            | BuildAction::BuildCore { target, .. }
            | BuildAction::LinkCore { target, .. }
            | BuildAction::MakeExecutable { target, .. }
            | BuildAction::GenerateDsym { target, .. }
            | BuildAction::GenerateTestInfo { target, .. }
            | BuildAction::GenerateMbti { target } => Some(target.package),
            BuildAction::BuildCStub { package, .. }
            | BuildAction::ArchiveOrLinkCStubs { package, .. }
            | BuildAction::BuildVirtual { package }
            | BuildAction::RunPrebuild { package, .. }
            | BuildAction::RunMoonLexPrebuild { package, .. }
            | BuildAction::RunMoonYaccPrebuild { package, .. } => Some(package),
            BuildAction::Bundle { .. }
            | BuildAction::BuildRuntimeObject { .. }
            | BuildAction::BuildRuntimeLib { .. }
            | BuildAction::BuildDocs { .. } => None,
        };
        let script_owned_actions = self
            .action_ids()
            .filter(|&action| action_package(self.action(action)) == Some(script_package))
            .collect::<HashSet<_>>();
        assert!(
            !script_owned_actions.is_empty(),
            "standalone action plan should contain work for the synthesized script package"
        );

        let mut dependency_actions = self
            .action_ids()
            .filter(|&action| {
                action_package(self.action(action)).is_some_and(|package| package != script_package)
            })
            .collect::<HashSet<_>>();
        let mut pending = dependency_actions.iter().copied().collect::<Vec<_>>();
        while let Some(action) = pending.pop() {
            for dependency in self.dependency_action_ids(action) {
                assert!(
                    !script_owned_actions.contains(&dependency),
                    "standalone dependency preparation action {action:?} depends on \
                     script action {dependency:?}"
                );
                if dependency_actions.insert(dependency) {
                    pending.push(dependency);
                }
            }
        }
        assert!(
            self.input_action_ids()
                .iter()
                .all(|action| !dependency_actions.contains(action)),
            "standalone root action should remain in the script execution phase"
        );

        let dependencies = self
            .action_ids()
            .filter(|action| dependency_actions.contains(action))
            .collect();
        let script = self
            .action_ids()
            .filter(|action| !dependency_actions.contains(action))
            .collect();
        (dependencies, script)
    }

    pub fn action(&self, id: BuildActionId) -> BuildAction<'a> {
        let node = self.node(id);
        match node {
            BuildPlanNode::Check(target) => BuildAction::Check {
                target,
                info: self
                    .plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for Check nodes"),
            },
            BuildPlanNode::EmitProof(target) => BuildAction::EmitProof {
                target,
                info: self
                    .plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for EmitProof nodes"),
            },
            BuildPlanNode::Prove(target) => BuildAction::Prove {
                target,
                info: self
                    .plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for Prove nodes"),
            },
            BuildPlanNode::BuildCore(target) => BuildAction::BuildCore {
                target,
                info: self
                    .plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for BuildCore nodes"),
            },
            BuildPlanNode::BuildCStub(package, index) => BuildAction::BuildCStub {
                package,
                index,
                info: self
                    .plan
                    .get_c_stubs_info(package)
                    .expect("C stub info should be present for BuildCStub nodes"),
            },
            BuildPlanNode::ArchiveOrLinkCStubs(package) => BuildAction::ArchiveOrLinkCStubs {
                package,
                info: self
                    .plan
                    .get_c_stubs_info(package)
                    .expect("C stubs info should be present for BuildCStubs nodes"),
            },
            BuildPlanNode::LinkCore(target) => BuildAction::LinkCore {
                target,
                info: self
                    .plan
                    .get_link_core_info(&target)
                    .expect("Link core info should be present for LinkCore nodes"),
                make_executable_info: self.plan.get_make_executable_info(&target),
            },
            BuildPlanNode::MakeExecutable(target) => BuildAction::MakeExecutable {
                target,
                info: self.plan.get_make_executable_info(&target),
            },
            BuildPlanNode::GenerateDsym(target) => BuildAction::GenerateDsym {
                target,
                dsymutil: self
                    .plan
                    .get_dsymutil()
                    .expect("dsymutil should be present for GenerateDsym nodes"),
            },
            BuildPlanNode::GenerateTestInfo(target) => BuildAction::GenerateTestInfo {
                target,
                info: self
                    .plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for GenerateTestInfo nodes"),
            },
            BuildPlanNode::GenerateMbti(target) => BuildAction::GenerateMbti { target },
            BuildPlanNode::BuildVirtual(package) => BuildAction::BuildVirtual { package },
            BuildPlanNode::Bundle(module) => BuildAction::Bundle {
                module,
                targets: &self
                    .plan
                    .bundle_info(module)
                    .expect("Bundle info should be present when lowering bundle node")
                    .bundle_targets,
            },
            BuildPlanNode::BuildRuntimeObject(index) => BuildAction::BuildRuntimeObject {
                index,
                info: self
                    .plan
                    .get_runtime_info()
                    .expect("Runtime info should be present for runtime object nodes"),
            },
            BuildPlanNode::BuildRuntimeLib => BuildAction::BuildRuntimeLib {
                info: self
                    .plan
                    .get_runtime_info()
                    .expect("Runtime info should be present for BuildRuntimeLib nodes"),
            },
            BuildPlanNode::BuildDocs(module) => BuildAction::BuildDocs { module },
            BuildPlanNode::RunPrebuild(package, index) => BuildAction::RunPrebuild {
                package,
                index,
                info: self
                    .plan
                    .get_prebuild_info(package, index)
                    .expect("Prebuild info should be populated before lowering run prebuild"),
            },
            BuildPlanNode::RunMoonLexPrebuild(package, index) => {
                BuildAction::RunMoonLexPrebuild { package, index }
            }
            BuildPlanNode::RunMoonYaccPrebuild(package, index) => {
                BuildAction::RunMoonYaccPrebuild { package, index }
            }
        }
    }

    pub fn dependency_products(&self, id: BuildActionId) -> Vec<(BuildActionId, BuildProduct)> {
        let node = self.node(id);
        let file_dependencies = self.plan.file_dependencies(node).flat_map(|(node, kind)| {
            let dependency_action = self.id_for_node(node);
            self.products_for_edge(node, kind)
                .into_iter()
                .map(move |product| (dependency_action, product))
        });
        let artifact_dependencies =
            self.plan
                .artifact_dependencies(node)
                .map(|(provider, artifact)| {
                    (self.id_for_node(provider), BuildProduct::Artifact(artifact))
                });
        let mut seen = HashSet::new();
        file_dependencies
            .chain(artifact_dependencies)
            .filter(move |dependency| seen.insert(dependency.clone()))
            .collect()
    }

    pub(crate) fn dependency_action_ids(
        &self,
        id: BuildActionId,
    ) -> impl Iterator<Item = BuildActionId> + '_ {
        self.plan
            .dependency_nodes(self.node(id))
            .map(|node| self.id_for_node(node))
    }

    pub fn output_products(&self, id: BuildActionId) -> Vec<BuildProduct> {
        self.output_products_for_node(self.node(id))
    }

    pub fn fileloc(
        &self,
        id: BuildActionId,
        modules: &ResolvedEnv,
        packages: &DiscoverResult,
    ) -> String {
        self.node(id).string_id(modules, packages)
    }

    pub fn human_desc(
        &self,
        id: BuildActionId,
        modules: &ResolvedEnv,
        packages: &DiscoverResult,
    ) -> String {
        self.node(id).human_desc(modules, packages)
    }

    pub fn package_for_error(&self, id: BuildActionId) -> Option<BuildTarget> {
        self.node(id).extract_target()
    }

    pub fn can_dirty_on_output(&self, id: BuildActionId) -> bool {
        matches!(
            self.node(id),
            BuildPlanNode::Check(_) | BuildPlanNode::EmitProof(_) | BuildPlanNode::Prove(_)
        )
    }

    pub(crate) fn generates_dsym_for_target(&self, target: &BuildTarget) -> bool {
        self.action_ids_by_node
            .contains_key(&BuildPlanNode::GenerateDsym(*target))
    }

    pub fn build_plan_node(&self, id: BuildActionId) -> BuildPlanNode {
        self.node(id)
    }

    pub(super) fn id_for_node(&self, node: BuildPlanNode) -> BuildActionId {
        *self
            .action_ids_by_node
            .get(&node)
            .expect("node should be present in build action plan")
    }

    fn node(&self, id: BuildActionId) -> BuildPlanNode {
        self.action_nodes[id.0]
    }

    fn products_for_edge(
        &self,
        node: BuildPlanNode,
        kind: FileDependencyKind,
    ) -> Vec<BuildProduct> {
        match kind {
            FileDependencyKind::AllFiles => self.output_products_for_node(node),
            FileDependencyKind::ProofArtifacts { mi, mlw, report } => {
                let mut products = Vec::new();
                if mi {
                    self.push_proof_interface(node, &mut products);
                }
                if mlw {
                    self.push_proof_whyml(node, &mut products);
                }
                if report {
                    self.push_proof_report(node, &mut products);
                }
                products
            }
            FileDependencyKind::GenerateTestInfo { meta } => {
                let mut products = Vec::new();
                self.push_generated_test_driver(node, &mut products);
                if meta {
                    self.push_generated_test_metadata(node, &mut products);
                }
                products
            }
        }
    }

    fn output_products_for_node(&self, node: BuildPlanNode) -> Vec<BuildProduct> {
        match node {
            BuildPlanNode::Check(target) => vec![BuildProduct::Artifact(ArtifactKey::CheckMi {
                package: target.package,
                target_kind: target.kind,
            })],
            BuildPlanNode::EmitProof(target) => vec![
                BuildProduct::ProofInterface { target },
                BuildProduct::ProofWhyml { target },
            ],
            BuildPlanNode::Prove(target) => vec![
                BuildProduct::ProofInterface { target },
                BuildProduct::ProofWhyml { target },
                BuildProduct::ProofReport { target },
            ],
            BuildPlanNode::BuildCore(target) => {
                let mut products = Vec::new();
                self.push_build_core_interface_if_emitted(target, &mut products);
                products.push(BuildProduct::Artifact(ArtifactKey::CoreIr {
                    package: target.package,
                    target_kind: target.kind,
                }));
                products
            }
            BuildPlanNode::BuildCStub(package, index) => {
                vec![BuildProduct::CStubObject { package, index }]
            }
            BuildPlanNode::ArchiveOrLinkCStubs(package) => {
                vec![BuildProduct::CStubLibrary { package }]
            }
            BuildPlanNode::LinkCore(target) => {
                vec![BuildProduct::LinkedCore { target }]
            }
            BuildPlanNode::MakeExecutable(target) => {
                vec![BuildProduct::Executable { target }]
            }
            BuildPlanNode::GenerateDsym(target) => {
                vec![BuildProduct::DsymBundle { target }]
            }
            BuildPlanNode::GenerateTestInfo(target) => vec![
                BuildProduct::GeneratedTestDriver { target },
                BuildProduct::GeneratedTestMetadata { target },
            ],
            BuildPlanNode::Bundle(module) => {
                vec![BuildProduct::BundleResult { module }]
            }
            BuildPlanNode::BuildRuntimeObject(index) => {
                vec![BuildProduct::RuntimeObject { index }]
            }
            BuildPlanNode::BuildRuntimeLib => vec![BuildProduct::RuntimeLib],
            BuildPlanNode::GenerateMbti(target) => {
                vec![BuildProduct::GeneratedMbti { target }]
            }
            BuildPlanNode::BuildDocs(_) => vec![BuildProduct::DocsDir],
            BuildPlanNode::RunPrebuild(package, index) => self
                .plan
                .get_prebuild_info(package, index)
                .expect("Prebuild info should be populated before lowering run prebuild")
                .resolved_outputs
                .iter()
                .cloned()
                .map(|path| BuildProduct::PrebuildOutputPath { path })
                .collect(),
            BuildPlanNode::BuildVirtual(package) => {
                vec![BuildProduct::Artifact(ArtifactKey::VirtualContractMi {
                    package,
                })]
            }
            BuildPlanNode::RunMoonLexPrebuild(package, index) => {
                vec![BuildProduct::MoonLexGeneratedSource { package, index }]
            }
            BuildPlanNode::RunMoonYaccPrebuild(package, index) => {
                vec![BuildProduct::MoonYaccGeneratedSource { package, index }]
            }
        }
    }

    fn push_build_core_interface_if_emitted(
        &self,
        target: BuildTarget,
        products: &mut Vec<BuildProduct>,
    ) {
        let info = self
            .plan
            .get_build_target_info(&target)
            .expect("Build target info should be present for BuildCore nodes");
        if info.check_mi_against.is_none() && !info.no_mi() && !target.kind.is_test() {
            products.push(BuildProduct::Artifact(ArtifactKey::BuildMi {
                package: target.package,
                target_kind: target.kind,
            }));
        }
    }

    fn push_proof_interface(&self, node: BuildPlanNode, products: &mut Vec<BuildProduct>) {
        match node {
            BuildPlanNode::EmitProof(target) | BuildPlanNode::Prove(target) => {
                products.push(BuildProduct::ProofInterface { target });
            }
            _ => panic!("Proof interface product requested from non-proof node"),
        }
    }

    fn push_proof_whyml(&self, node: BuildPlanNode, products: &mut Vec<BuildProduct>) {
        match node {
            BuildPlanNode::EmitProof(target) | BuildPlanNode::Prove(target) => {
                products.push(BuildProduct::ProofWhyml { target });
            }
            _ => panic!("Proof WhyML product requested from non-proof node"),
        }
    }

    fn push_proof_report(&self, node: BuildPlanNode, products: &mut Vec<BuildProduct>) {
        match node {
            BuildPlanNode::Prove(target) => {
                products.push(BuildProduct::ProofReport { target });
            }
            BuildPlanNode::EmitProof(_) => {}
            _ => panic!("Proof report product requested from non-proof node"),
        }
    }

    fn push_generated_test_driver(&self, node: BuildPlanNode, products: &mut Vec<BuildProduct>) {
        match node {
            BuildPlanNode::GenerateTestInfo(target) => {
                products.push(BuildProduct::GeneratedTestDriver { target });
            }
            _ => panic!("Test driver product requested from non-test-info node"),
        }
    }

    fn push_generated_test_metadata(&self, node: BuildPlanNode, products: &mut Vec<BuildProduct>) {
        match node {
            BuildPlanNode::GenerateTestInfo(target) => {
                products.push(BuildProduct::GeneratedTestMetadata { target });
            }
            _ => panic!("Test metadata product requested from non-test-info node"),
        }
    }
}
