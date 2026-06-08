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

use std::collections::HashMap;

use moonutil::mooncakes::result::ResolvedEnv;

use crate::{
    build_plan::{BuildPlan, FileDependencyKind, PlanArtifactKind},
    discover::DiscoverResult,
    model::{BuildPlanNode, BuildTarget},
};

use super::{BuildAction, BuildActionId, PlannedArtifact};

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
            BuildPlanNode::BuildRuntimeLib => BuildAction::BuildRuntimeLib,
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

    pub fn dependency_artifacts(&self, id: BuildActionId) -> Vec<PlannedArtifact> {
        self.plan
            .dependency_edges(self.node(id))
            .flat_map(|(node, kind)| self.artifacts_for_edge(node, kind))
            .collect()
    }

    pub fn output_artifacts(&self, id: BuildActionId) -> Vec<PlannedArtifact> {
        self.output_artifacts_for_node(self.node(id))
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

    pub fn needs_moonc_tool_dep(&self, id: BuildActionId) -> bool {
        matches!(
            self.node(id),
            BuildPlanNode::Check(_)
                | BuildPlanNode::EmitProof(_)
                | BuildPlanNode::Prove(_)
                | BuildPlanNode::BuildCore(_)
                | BuildPlanNode::LinkCore(_)
                | BuildPlanNode::BuildVirtual(_)
                | BuildPlanNode::Bundle(_)
        )
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

    fn artifacts_for_edge(
        &self,
        node: BuildPlanNode,
        kind: FileDependencyKind,
    ) -> Vec<PlannedArtifact> {
        match kind {
            FileDependencyKind::AllFiles => self.output_artifacts_for_node(node),
            FileDependencyKind::Artifacts(need) => {
                let mut artifacts = Vec::new();
                if need.contains(PlanArtifactKind::Interface) {
                    self.push_package_interface(node, &mut artifacts);
                }
                if need.contains(PlanArtifactKind::CoreIr) {
                    self.push_package_core_ir(node, &mut artifacts);
                }
                artifacts
            }
            FileDependencyKind::ProofArtifacts { mi, mlw, report } => {
                let mut artifacts = Vec::new();
                if mi {
                    self.push_proof_interface(node, &mut artifacts);
                }
                if mlw {
                    self.push_proof_whyml(node, &mut artifacts);
                }
                if report {
                    self.push_proof_report(node, &mut artifacts);
                }
                artifacts
            }
            FileDependencyKind::GenerateTestInfo { meta } => {
                let mut artifacts = Vec::new();
                self.push_generated_test_driver(node, &mut artifacts);
                if meta {
                    self.push_generated_test_metadata(node, &mut artifacts);
                }
                artifacts
            }
        }
    }

    fn output_artifacts_for_node(&self, node: BuildPlanNode) -> Vec<PlannedArtifact> {
        let producer = self.id_for_node(node);
        match node {
            BuildPlanNode::Check(target) => {
                vec![PlannedArtifact::PackageInterface { producer, target }]
            }
            BuildPlanNode::EmitProof(target) => vec![
                PlannedArtifact::ProofInterface { producer, target },
                PlannedArtifact::ProofWhyml { producer, target },
            ],
            BuildPlanNode::Prove(target) => vec![
                PlannedArtifact::ProofInterface { producer, target },
                PlannedArtifact::ProofWhyml { producer, target },
                PlannedArtifact::ProofReport { producer, target },
            ],
            BuildPlanNode::BuildCore(target) => {
                let mut artifacts = Vec::new();
                self.push_build_core_interface_if_emitted(target, producer, &mut artifacts);
                artifacts.push(PlannedArtifact::PackageCoreIr { producer, target });
                artifacts
            }
            BuildPlanNode::BuildCStub(package, index) => {
                vec![PlannedArtifact::CStubObject {
                    producer,
                    package,
                    index,
                }]
            }
            BuildPlanNode::ArchiveOrLinkCStubs(package) => {
                vec![PlannedArtifact::CStubLibrary { producer, package }]
            }
            BuildPlanNode::LinkCore(target) => {
                vec![PlannedArtifact::LinkedCore { producer, target }]
            }
            BuildPlanNode::MakeExecutable(target) => {
                vec![PlannedArtifact::Executable { producer, target }]
            }
            BuildPlanNode::GenerateTestInfo(target) => vec![
                PlannedArtifact::GeneratedTestDriver { producer, target },
                PlannedArtifact::GeneratedTestMetadata { producer, target },
            ],
            BuildPlanNode::Bundle(module) => {
                vec![PlannedArtifact::BundleResult { producer, module }]
            }
            BuildPlanNode::BuildRuntimeLib => vec![PlannedArtifact::RuntimeLib { producer }],
            BuildPlanNode::GenerateMbti(target) => {
                vec![PlannedArtifact::GeneratedMbti { producer, target }]
            }
            BuildPlanNode::BuildDocs(_) => vec![PlannedArtifact::DocsDir { producer }],
            BuildPlanNode::RunPrebuild(package, index) => self
                .plan
                .get_prebuild_info(package, index)
                .expect("Prebuild info should be populated before lowering run prebuild")
                .resolved_outputs
                .iter()
                .cloned()
                .map(|path| PlannedArtifact::KnownPath { producer, path })
                .collect(),
            BuildPlanNode::BuildVirtual(package) => {
                vec![PlannedArtifact::VirtualPackageInterface { producer, package }]
            }
            BuildPlanNode::RunMoonLexPrebuild(package, index) => {
                vec![PlannedArtifact::MoonLexGeneratedSource {
                    producer,
                    package,
                    index,
                }]
            }
            BuildPlanNode::RunMoonYaccPrebuild(package, index) => {
                vec![PlannedArtifact::MoonYaccGeneratedSource {
                    producer,
                    package,
                    index,
                }]
            }
        }
    }

    fn push_package_interface(&self, node: BuildPlanNode, artifacts: &mut Vec<PlannedArtifact>) {
        let producer = self.id_for_node(node);
        match node {
            BuildPlanNode::Check(target) => {
                artifacts.push(PlannedArtifact::PackageInterface { producer, target });
            }
            BuildPlanNode::BuildCore(target) => {
                self.push_build_core_interface_if_emitted(target, producer, artifacts);
            }
            _ => panic!("Package interface artifact requested from non-package producer"),
        }
    }

    fn push_package_core_ir(&self, node: BuildPlanNode, artifacts: &mut Vec<PlannedArtifact>) {
        let producer = self.id_for_node(node);
        match node {
            BuildPlanNode::BuildCore(target) => {
                artifacts.push(PlannedArtifact::PackageCoreIr { producer, target });
            }
            _ => panic!("Core IR artifact requested from non-BuildCore producer"),
        }
    }

    fn push_build_core_interface_if_emitted(
        &self,
        target: BuildTarget,
        producer: BuildActionId,
        artifacts: &mut Vec<PlannedArtifact>,
    ) {
        let info = self
            .plan
            .get_build_target_info(&target)
            .expect("Build target info should be present for BuildCore nodes");
        if info.check_mi_against.is_none() && !info.no_mi() && !target.kind.is_test() {
            artifacts.push(PlannedArtifact::PackageInterface { producer, target });
        }
    }

    fn push_proof_interface(&self, node: BuildPlanNode, artifacts: &mut Vec<PlannedArtifact>) {
        let producer = self.id_for_node(node);
        match node {
            BuildPlanNode::EmitProof(target) | BuildPlanNode::Prove(target) => {
                artifacts.push(PlannedArtifact::ProofInterface { producer, target });
            }
            _ => panic!("Proof interface artifact requested from non-proof producer"),
        }
    }

    fn push_proof_whyml(&self, node: BuildPlanNode, artifacts: &mut Vec<PlannedArtifact>) {
        let producer = self.id_for_node(node);
        match node {
            BuildPlanNode::EmitProof(target) | BuildPlanNode::Prove(target) => {
                artifacts.push(PlannedArtifact::ProofWhyml { producer, target });
            }
            _ => panic!("Proof WhyML artifact requested from non-proof producer"),
        }
    }

    fn push_proof_report(&self, node: BuildPlanNode, artifacts: &mut Vec<PlannedArtifact>) {
        let producer = self.id_for_node(node);
        match node {
            BuildPlanNode::Prove(target) => {
                artifacts.push(PlannedArtifact::ProofReport { producer, target });
            }
            BuildPlanNode::EmitProof(_) => {}
            _ => panic!("Proof report artifact requested from non-proof producer"),
        }
    }

    fn push_generated_test_driver(
        &self,
        node: BuildPlanNode,
        artifacts: &mut Vec<PlannedArtifact>,
    ) {
        let producer = self.id_for_node(node);
        match node {
            BuildPlanNode::GenerateTestInfo(target) => {
                artifacts.push(PlannedArtifact::GeneratedTestDriver { producer, target });
            }
            _ => panic!("Test driver artifact requested from non-test-info producer"),
        }
    }

    fn push_generated_test_metadata(
        &self,
        node: BuildPlanNode,
        artifacts: &mut Vec<PlannedArtifact>,
    ) {
        let producer = self.id_for_node(node);
        match node {
            BuildPlanNode::GenerateTestInfo(target) => {
                artifacts.push(PlannedArtifact::GeneratedTestMetadata { producer, target });
            }
            _ => panic!("Test metadata artifact requested from non-test-info producer"),
        }
    }
}
