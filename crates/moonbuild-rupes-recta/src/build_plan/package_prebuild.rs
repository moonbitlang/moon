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

use crate::model::{BuildPlanNode, PackageId};

/// Resolved information about a custom package prebuild command.
#[derive(Debug)]
pub struct PrebuildInfo {
    pub(crate) resolved_inputs: Vec<PathBuf>,
    pub(crate) resolved_outputs: Vec<PathBuf>,
    /// Workspaces may contain packages from different modules, so the module
    /// root is part of each resolved command rather than global plan state.
    pub(crate) cwd: PathBuf,
    pub(crate) command: String,
}

#[derive(Debug)]
pub(crate) enum PackagePrebuildAction {
    Custom {
        package: PackageId,
        index: u32,
        info: PrebuildInfo,
    },
    MoonLex {
        package: PackageId,
        index: u32,
        input: PathBuf,
        output: PathBuf,
    },
    MoonYacc {
        package: PackageId,
        index: u32,
        input: PathBuf,
        output: PathBuf,
    },
}

impl PackagePrebuildAction {
    pub(crate) fn node(&self) -> BuildPlanNode {
        match self {
            Self::Custom { package, index, .. } => BuildPlanNode::RunPrebuild(*package, *index),
            Self::MoonLex { package, index, .. } => {
                BuildPlanNode::RunMoonLexPrebuild(*package, *index)
            }
            Self::MoonYacc { package, index, .. } => {
                BuildPlanNode::RunMoonYaccPrebuild(*package, *index)
            }
        }
    }

    pub(crate) fn package(&self) -> PackageId {
        match self {
            Self::Custom { package, .. }
            | Self::MoonLex { package, .. }
            | Self::MoonYacc { package, .. } => *package,
        }
    }

    pub(crate) fn output_paths(&self) -> &[PathBuf] {
        match self {
            Self::Custom { info, .. } => &info.resolved_outputs,
            Self::MoonLex { output, .. } | Self::MoonYacc { output, .. } => {
                std::slice::from_ref(output)
            }
        }
    }

    pub(crate) fn input_paths(&self) -> &[PathBuf] {
        match self {
            Self::Custom { info, .. } => &info.resolved_inputs,
            Self::MoonLex { input, .. } | Self::MoonYacc { input, .. } => {
                std::slice::from_ref(input)
            }
        }
    }
}

/// Backend-independent package prebuild actions within one Build Plan.
///
/// Actions are stored separately from backend actions, but use the same
/// artifact provider/requirement registry. Each action is complete: its command
/// and physical inputs/outputs travel together. The existing `(package,
/// index)` node is only a compatibility address used by Build Action
/// Projection.
#[derive(Default)]
pub(crate) struct PackagePrebuildPlan {
    actions: Vec<PackagePrebuildAction>,
}

impl PackagePrebuildPlan {
    pub(crate) fn action_count(&self) -> usize {
        self.actions.len()
    }

    pub(crate) fn nodes(&self) -> impl Iterator<Item = BuildPlanNode> + '_ {
        self.actions.iter().map(PackagePrebuildAction::node)
    }

    pub(crate) fn contains_node(&self, node: BuildPlanNode) -> bool {
        self.actions.iter().any(|action| action.node() == node)
    }

    pub(crate) fn action(&self, node: BuildPlanNode) -> Option<&PackagePrebuildAction> {
        self.actions.iter().find(|action| action.node() == node)
    }

    pub(crate) fn actions_for_package(
        &self,
        package: PackageId,
    ) -> impl Iterator<Item = &PackagePrebuildAction> {
        self.actions
            .iter()
            .filter(move |action| action.package() == package)
    }

    pub(crate) fn insert_custom(&mut self, package: PackageId, index: u32, info: PrebuildInfo) {
        let node = BuildPlanNode::RunPrebuild(package, index);
        debug_assert!(
            !self.contains_node(node),
            "custom prebuild should only be planned once"
        );
        self.actions.push(PackagePrebuildAction::Custom {
            package,
            index,
            info,
        });
    }

    pub(crate) fn insert_moonlex(
        &mut self,
        package: PackageId,
        index: u32,
        input: PathBuf,
        output: PathBuf,
    ) {
        let node = BuildPlanNode::RunMoonLexPrebuild(package, index);
        if self.contains_node(node) {
            return;
        }
        self.actions.push(PackagePrebuildAction::MoonLex {
            package,
            index,
            input,
            output,
        });
    }

    pub(crate) fn insert_moonyacc(
        &mut self,
        package: PackageId,
        index: u32,
        input: PathBuf,
        output: PathBuf,
    ) {
        let node = BuildPlanNode::RunMoonYaccPrebuild(package, index);
        if self.contains_node(node) {
            return;
        }
        self.actions.push(PackagePrebuildAction::MoonYacc {
            package,
            index,
            input,
            output,
        });
    }

    /// All concrete outputs produced for one package, in planning order.
    pub(crate) fn output_paths(&self, package: PackageId) -> impl Iterator<Item = &PathBuf> {
        self.actions
            .iter()
            .filter(move |action| action.package() == package)
            .flat_map(PackagePrebuildAction::output_paths)
    }

    pub(crate) fn custom_output_paths(&self, package: PackageId) -> impl Iterator<Item = &PathBuf> {
        self.actions.iter().flat_map(move |action| match action {
            PackagePrebuildAction::Custom {
                package: action_package,
                info,
                ..
            } if *action_package == package => info.resolved_outputs.as_slice(),
            _ => &[],
        })
    }

    #[cfg(test)]
    pub(crate) fn test_insert_custom_info(
        &mut self,
        package: PackageId,
        infos: Vec<Option<PrebuildInfo>>,
    ) {
        for (index, info) in infos.into_iter().enumerate() {
            if let Some(info) = info {
                let node = BuildPlanNode::RunPrebuild(package, index as u32);
                let action = PackagePrebuildAction::Custom {
                    package,
                    index: index as u32,
                    info,
                };
                if let Some(existing) = self.actions.iter_mut().find(|action| action.node() == node)
                {
                    *existing = action;
                } else {
                    self.actions.push(action);
                }
            }
        }
    }
}

pub(crate) fn is_package_prebuild_node(node: BuildPlanNode) -> bool {
    matches!(
        node,
        BuildPlanNode::RunPrebuild(..)
            | BuildPlanNode::RunMoonLexPrebuild(..)
            | BuildPlanNode::RunMoonYaccPrebuild(..)
    )
}

#[cfg(test)]
mod tests {
    use slotmap::KeyData;

    use crate::model::TargetKind;

    use super::*;
    use crate::build_plan::BuildPlan;

    fn package_id(raw: u64) -> PackageId {
        PackageId::from(KeyData::from_ffi(raw))
    }

    #[test]
    fn package_prebuild_provider_is_separate_from_backend_graph() {
        let package = package_id(1);
        let backend_node = BuildPlanNode::Check(package.build_target(TargetKind::Source));
        let prebuild_node = BuildPlanNode::RunPrebuild(package, 0);
        let mut plan = BuildPlan::default();
        plan.actions.insert(backend_node);
        plan.package_prebuild.insert_custom(
            package,
            0,
            PrebuildInfo {
                resolved_inputs: vec![PathBuf::from("input.txt")],
                resolved_outputs: vec![PathBuf::from("generated.mbt")],
                cwd: PathBuf::from("."),
                command: "generate".to_string(),
            },
        );

        assert!(plan.actions.contains(&backend_node));
        assert!(!plan.actions.contains(&prebuild_node));
        assert_eq!(plan.package_prebuild_plan().action_count(), 1);
        assert_eq!(
            plan.all_nodes().collect::<Vec<_>>(),
            [backend_node, prebuild_node]
        );
        assert_eq!(plan.dependency_nodes(backend_node).count(), 0);
    }
}
