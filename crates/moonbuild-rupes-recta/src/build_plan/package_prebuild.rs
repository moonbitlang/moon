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

use indexmap::IndexMap;

use crate::model::PackageId;

/// Identity of one package-level file-generation action.
///
/// Custom commands have no declared name and may intentionally have no output,
/// so their manifest position is their only lossless declaration coordinate.
/// Built-in generators are instead identified by their concrete input path.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PackagePrebuildKey {
    Custom {
        package: PackageId,
        declaration_index: u32,
    },
    MoonLex {
        package: PackageId,
        input: PathBuf,
    },
    MoonYacc {
        package: PackageId,
        input: PathBuf,
    },
}

impl PackagePrebuildKey {
    pub(crate) fn package(&self) -> PackageId {
        match self {
            Self::Custom { package, .. }
            | Self::MoonLex { package, .. }
            | Self::MoonYacc { package, .. } => *package,
        }
    }
}

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
    Custom { info: PrebuildInfo },
    MoonLex { input: PathBuf, output: PathBuf },
    MoonYacc { input: PathBuf, output: PathBuf },
}

impl PackagePrebuildAction {
    pub(crate) fn output_paths(&self) -> &[PathBuf] {
        match self {
            Self::Custom { info, .. } => &info.resolved_outputs,
            Self::MoonLex { output, .. } | Self::MoonYacc { output, .. } => {
                std::slice::from_ref(output)
            }
        }
    }
}

/// The backend-independent package-prebuild subplan within one Build Plan.
///
/// Each key names exactly one action. Command data and physical inputs and
/// outputs remain with that action; Execution Plan construction connects them
/// to other actions by matching concrete paths.
#[derive(Default)]
pub(crate) struct PackagePrebuildPlan {
    actions: IndexMap<PackagePrebuildKey, PackagePrebuildAction>,
}

impl PackagePrebuildPlan {
    pub(crate) fn action_count(&self) -> usize {
        self.actions.len()
    }

    pub(crate) fn actions(
        &self,
    ) -> impl Iterator<Item = (&PackagePrebuildKey, &PackagePrebuildAction)> {
        self.actions.iter()
    }

    pub(crate) fn contains_key(&self, key: &PackagePrebuildKey) -> bool {
        self.actions.contains_key(key)
    }

    pub(crate) fn action(&self, key: &PackagePrebuildKey) -> Option<&PackagePrebuildAction> {
        self.actions.get(key)
    }

    pub(crate) fn insert_custom(&mut self, package: PackageId, index: u32, info: PrebuildInfo) {
        let key = PackagePrebuildKey::Custom {
            package,
            declaration_index: index,
        };
        let previous = self
            .actions
            .insert(key, PackagePrebuildAction::Custom { info });
        debug_assert!(
            previous.is_none(),
            "custom prebuild should only be planned once"
        );
    }

    pub(crate) fn insert_moonlex(&mut self, package: PackageId, input: PathBuf, output: PathBuf) {
        let key = PackagePrebuildKey::MoonLex {
            package,
            input: input.clone(),
        };
        if self.contains_key(&key) {
            return;
        }
        self.actions
            .insert(key, PackagePrebuildAction::MoonLex { input, output });
    }

    pub(crate) fn insert_moonyacc(&mut self, package: PackageId, input: PathBuf, output: PathBuf) {
        let key = PackagePrebuildKey::MoonYacc {
            package,
            input: input.clone(),
        };
        if self.contains_key(&key) {
            return;
        }
        self.actions
            .insert(key, PackagePrebuildAction::MoonYacc { input, output });
    }

    /// All concrete outputs produced for one package, in planning order.
    pub(crate) fn output_paths(&self, package: PackageId) -> impl Iterator<Item = &PathBuf> {
        self.actions
            .iter()
            .filter(move |(key, _)| key.package() == package)
            .flat_map(|(_, action)| action.output_paths())
    }

    pub(crate) fn custom_output_paths(&self, package: PackageId) -> impl Iterator<Item = &PathBuf> {
        self.actions
            .iter()
            .flat_map(move |(key, action)| match (key, action) {
                (
                    PackagePrebuildKey::Custom {
                        package: action_package,
                        ..
                    },
                    PackagePrebuildAction::Custom { info },
                ) if *action_package == package => info.resolved_outputs.as_slice(),
                _ => &[],
            })
    }
}

#[cfg(test)]
mod tests {
    use slotmap::KeyData;

    use crate::model::{BuildPlanNode, TargetKind};

    use super::*;
    use crate::build_plan::{BuildPlan, BuildPlanActionKey};

    fn package_id(raw: u64) -> PackageId {
        PackageId::from(KeyData::from_ffi(raw))
    }

    #[test]
    fn package_prebuild_action_is_separate_from_backend_plan() {
        let package = package_id(1);
        let backend_node = BuildPlanNode::Check(package.build_target(TargetKind::Source));
        let prebuild_key = PackagePrebuildKey::Custom {
            package,
            declaration_index: 0,
        };
        let mut plan = BuildPlan::default();
        plan.backend.insert(backend_node);
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

        assert!(plan.backend.contains(&backend_node));
        assert_eq!(plan.package_prebuild.action_count(), 1);
        assert!(plan.package_prebuild.contains_key(&prebuild_key));
        let backend = BuildPlanActionKey::Backend(backend_node);
        assert_eq!(
            plan.all_actions().collect::<Vec<_>>(),
            [
                backend.clone(),
                BuildPlanActionKey::PackagePrebuild(prebuild_key),
            ]
        );
        assert_eq!(plan.dependency_actions(&backend).count(), 0);
    }
}
