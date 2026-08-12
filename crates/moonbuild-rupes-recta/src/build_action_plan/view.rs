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
    build_plan::{ArtifactKey, BuildPlan, PackagePrebuildAction},
    discover::DiscoverResult,
    model::{BuildPlanNode, BuildTarget, PackageId},
};

use super::{BuildAction, BuildActionId};

/// Normalized action-level view consumed by backend lowering.
pub struct BuildActionPlan<'a> {
    plan: &'a BuildPlan,
    action_nodes: Vec<BuildPlanNode>,
    action_ids_by_node: HashMap<BuildPlanNode, BuildActionId>,
    requested_artifacts: Vec<(ArtifactKey, BuildActionId)>,
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
        let requested_artifacts = self
            .requested_artifacts()
            .map(|artifact| {
                let provider = self.artifact_provider(artifact);
                let action = *action_ids_by_node
                    .get(&provider)
                    .expect("artifact provider should be present in build action plan");
                (artifact.clone(), action)
            })
            .collect();
        BuildActionPlan {
            plan: self,
            action_nodes,
            action_ids_by_node,
            requested_artifacts,
        }
    }
}

impl<'a> BuildActionPlan<'a> {
    pub fn action_ids(&self) -> impl Iterator<Item = BuildActionId> + '_ {
        (0..self.action_nodes.len()).map(BuildActionId)
    }

    pub fn requested_artifacts(&self) -> &[(ArtifactKey, BuildActionId)] {
        &self.requested_artifacts
    }

    pub fn root_action_ids(&self) -> impl Iterator<Item = BuildActionId> + '_ {
        let mut seen = HashSet::new();
        self.requested_artifacts
            .iter()
            .map(|(_, action)| *action)
            .filter(move |action| seen.insert(*action))
    }

    /// Separate reusable package preparation from work owned by the
    /// synthesized script package while preserving the original action edges.
    pub(crate) fn partition_standalone_actions(
        &self,
        script_package: PackageId,
    ) -> (Vec<BuildActionId>, Vec<BuildActionId>) {
        let action_package = |node| match node {
            BuildPlanNode::Check(target)
            | BuildPlanNode::EmitProof(target)
            | BuildPlanNode::Prove(target)
            | BuildPlanNode::BuildCore(target)
            | BuildPlanNode::LinkCore(target)
            | BuildPlanNode::MakeExecutable(target)
            | BuildPlanNode::GenerateDsym(target)
            | BuildPlanNode::GenerateTestInfo(target)
            | BuildPlanNode::GenerateMbti(target) => Some(target.package),
            BuildPlanNode::BuildCStub(package, _)
            | BuildPlanNode::ArchiveOrLinkCStubs(package)
            | BuildPlanNode::BuildVirtual(package)
            | BuildPlanNode::RunPrebuild(package, _)
            | BuildPlanNode::RunMoonLexPrebuild(package, _)
            | BuildPlanNode::RunMoonYaccPrebuild(package, _) => Some(package),
            BuildPlanNode::Bundle(_)
            | BuildPlanNode::BuildRuntimeObject(_)
            | BuildPlanNode::BuildRuntimeLib
            | BuildPlanNode::BuildDocs(_) => None,
        };
        let script_owned_actions = self
            .action_ids()
            .filter(|&action| action_package(self.node(action)) == Some(script_package))
            .collect::<HashSet<_>>();
        assert!(
            !script_owned_actions.is_empty(),
            "standalone action plan should contain work for the synthesized script package"
        );

        let mut dependency_actions = self
            .action_ids()
            .filter(|&action| {
                action_package(self.node(action)).is_some_and(|package| package != script_package)
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
            self.root_action_ids()
                .all(|action| !dependency_actions.contains(&action)),
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
                info: self
                    .plan
                    .get_make_executable_info(&target)
                    .expect("MakeExecutable nodes should contain native linking info"),
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
            BuildPlanNode::BuildVirtual(package) => BuildAction::BuildVirtual {
                package,
                input: self
                    .plan
                    .virtual_contract_input(package)
                    .expect("virtual contract input should be selected during build planning"),
            },
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
            BuildPlanNode::RunPrebuild(package, index) => {
                let Some(PackagePrebuildAction::Custom { info, .. }) =
                    self.plan.package_prebuild_plan().action(node)
                else {
                    unreachable!("complete package prebuild actions contain their prebuild info");
                };
                BuildAction::RunPrebuild {
                    package,
                    index,
                    info,
                }
            }
            BuildPlanNode::RunMoonLexPrebuild(package, _) => {
                let Some(PackagePrebuildAction::MoonLex { input, output, .. }) =
                    self.plan.package_prebuild_plan().action(node)
                else {
                    unreachable!("moonlex actions contain their input and output paths")
                };
                BuildAction::RunMoonLexPrebuild {
                    package,
                    input,
                    output,
                }
            }
            BuildPlanNode::RunMoonYaccPrebuild(package, _) => {
                let Some(PackagePrebuildAction::MoonYacc { input, output, .. }) =
                    self.plan.package_prebuild_plan().action(node)
                else {
                    unreachable!("moonyacc actions contain their input and output paths")
                };
                BuildAction::RunMoonYaccPrebuild {
                    package,
                    input,
                    output,
                }
            }
        }
    }

    pub fn dependency_artifacts(&self, id: BuildActionId) -> Vec<(BuildActionId, ArtifactKey)> {
        let node = self.node(id);
        let mut seen = HashSet::new();
        self.plan
            .artifact_dependencies(node)
            .map(|(provider, artifact)| (self.id_for_node(provider), artifact))
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

    pub fn output_artifacts(&self, id: BuildActionId) -> Vec<ArtifactKey> {
        self.plan.provided_artifacts(self.node(id)).collect()
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
        // A custom prebuild may generate an `.mbl` or `.mby` that package
        // discovery never observed. Describe generator actions from their
        // resolved input paths rather than indexing the discovered file lists.
        let generator_desc = |tool: &str, package, input: &std::path::Path| {
            let input_name = input.file_name().map_or_else(
                || input.display().to_string(),
                |name| name.to_string_lossy().into(),
            );
            format!("run {tool} {} {input_name}", packages.fqn(package))
        };
        match self.action(id) {
            BuildAction::RunMoonLexPrebuild { package, input, .. } => {
                generator_desc("moonlex", package, input)
            }
            BuildAction::RunMoonYaccPrebuild { package, input, .. } => {
                generator_desc("moonyacc", package, input)
            }
            _ => self.node(id).human_desc(modules, packages),
        }
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
}
