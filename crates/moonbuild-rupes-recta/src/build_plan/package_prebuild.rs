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

use std::path::PathBuf;

use indexmap::IndexMap;

use crate::model::PackageId;

/// Identity of one package-level file-generation action.
///
/// Custom commands have no declared name and may intentionally have no output,
/// so their manifest position is their only lossless declaration coordinate.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PackagePrebuildKey {
    pub(crate) package: PackageId,
    pub(crate) declaration_index: u32,
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

/// The backend-independent package-prebuild subplan within one Build Plan.
///
/// Each key names exactly one action. Command data and physical inputs and
/// outputs remain with that action; Execution Plan construction connects them
/// to other actions by matching concrete paths.
#[derive(Default)]
pub(crate) struct PackagePrebuildPlan {
    actions: IndexMap<PackagePrebuildKey, PrebuildInfo>,
}

impl PackagePrebuildPlan {
    pub(crate) fn action_count(&self) -> usize {
        self.actions.len()
    }

    pub(crate) fn actions(&self) -> impl Iterator<Item = (&PackagePrebuildKey, &PrebuildInfo)> {
        self.actions.iter()
    }

    pub(crate) fn contains_key(&self, key: &PackagePrebuildKey) -> bool {
        self.actions.contains_key(key)
    }

    pub(crate) fn action(&self, key: &PackagePrebuildKey) -> Option<&PrebuildInfo> {
        self.actions.get(key)
    }

    pub(crate) fn insert_custom(&mut self, package: PackageId, index: u32, info: PrebuildInfo) {
        let key = PackagePrebuildKey {
            package,
            declaration_index: index,
        };
        let previous = self.actions.insert(key, info);
        debug_assert!(
            previous.is_none(),
            "custom prebuild should only be planned once"
        );
    }

    /// All concrete outputs produced for one package, in planning order.
    pub(crate) fn output_paths(&self, package: PackageId) -> impl Iterator<Item = &PathBuf> {
        self.actions
            .iter()
            .filter(move |(key, _)| key.package == package)
            .flat_map(|(_, action)| &action.resolved_outputs)
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
        let prebuild_key = PackagePrebuildKey {
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
