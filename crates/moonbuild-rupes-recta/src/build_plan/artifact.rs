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

use crate::model::{BuildPlanNode, PackageId, TargetKind};

/// The logical identity of a package compilation result within one build plan.
///
/// Physical output roots and provider derivations are intentionally absent.
/// Check, build, and virtual-contract interfaces are distinct because they are
/// not interchangeable inputs even though all three currently use `.mi` files.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ArtifactKey {
    CheckMi {
        package: PackageId,
        target_kind: TargetKind,
    },
    BuildMi {
        package: PackageId,
        target_kind: TargetKind,
    },
    CoreIr {
        package: PackageId,
        target_kind: TargetKind,
    },
    VirtualContractMi {
        package: PackageId,
    },
}

#[derive(Debug, Default)]
struct Derivation {
    requirements: HashSet<ArtifactKey>,
    outputs: HashSet<ArtifactKey>,
}

/// A normalized form of `artifact -> (requirements, provider)`.
///
/// Derivations are keyed by the existing high-level [`BuildPlanNode`]. This
/// lets multiple outputs share one requirement set without introducing a new
/// positional or arena ID.
#[derive(Debug, Default)]
pub(super) struct ArtifactPlan {
    providers: HashMap<ArtifactKey, BuildPlanNode>,
    derivations: HashMap<BuildPlanNode, Derivation>,
}

impl ArtifactPlan {
    pub(super) fn require(&mut self, consumer: BuildPlanNode, artifact: ArtifactKey) {
        self.derivations
            .entry(consumer)
            .or_default()
            .requirements
            .insert(artifact);
    }

    pub(super) fn provide(&mut self, provider: BuildPlanNode, artifact: ArtifactKey) {
        if let Some(&existing) = self.providers.get(&artifact) {
            assert_eq!(
                existing, provider,
                "artifact {artifact:?} has conflicting providers {existing:?} and {provider:?}"
            );
        } else {
            self.providers.insert(artifact, provider);
        }

        self.derivations
            .entry(provider)
            .or_default()
            .outputs
            .insert(artifact);
    }

    pub(super) fn provider(&self, artifact: ArtifactKey) -> Option<BuildPlanNode> {
        self.providers.get(&artifact).copied()
    }

    pub(super) fn requirements(&self) -> impl Iterator<Item = (BuildPlanNode, ArtifactKey)> + '_ {
        self.derivations.iter().flat_map(|(&consumer, derivation)| {
            derivation
                .requirements
                .iter()
                .copied()
                .map(move |artifact| (consumer, artifact))
        })
    }
}

#[cfg(test)]
mod tests {
    use slotmap::SlotMap;

    use super::*;

    #[test]
    fn multiple_artifacts_share_one_derivation() {
        let mut packages = SlotMap::<PackageId, ()>::with_key();
        let package = packages.insert(());
        let target = package.build_target(TargetKind::Source);
        let provider = BuildPlanNode::BuildCore(target);
        let build_mi = ArtifactKey::BuildMi {
            package,
            target_kind: TargetKind::Source,
        };
        let core_ir = ArtifactKey::CoreIr {
            package,
            target_kind: TargetKind::Source,
        };

        let mut plan = ArtifactPlan::default();
        plan.provide(provider, build_mi);
        plan.provide(provider, core_ir);

        assert_eq!(plan.provider(build_mi), Some(provider));
        assert_eq!(plan.provider(core_ir), Some(provider));
        assert_eq!(plan.derivations.len(), 1);
        assert_eq!(plan.derivations[&provider].outputs.len(), 2);
    }

    #[test]
    fn requirements_name_artifacts_without_providers() {
        let mut packages = SlotMap::<PackageId, ()>::with_key();
        let dependency = packages.insert(());
        let consumer = packages.insert(());
        let consumer = BuildPlanNode::BuildVirtual(consumer);
        let check_mi = ArtifactKey::CheckMi {
            package: dependency,
            target_kind: TargetKind::Source,
        };
        let build_mi = ArtifactKey::BuildMi {
            package: dependency,
            target_kind: TargetKind::Source,
        };

        let mut plan = ArtifactPlan::default();
        plan.require(consumer, build_mi);

        assert_eq!(
            plan.requirements().collect::<HashSet<_>>(),
            HashSet::from([(consumer, build_mi)])
        );
        assert_eq!(plan.provider(build_mi), None);
        assert_ne!(check_mi, build_mi);
    }

    #[test]
    #[should_panic(expected = "has conflicting providers")]
    fn one_artifact_cannot_have_two_providers() {
        let mut packages = SlotMap::<PackageId, ()>::with_key();
        let package = packages.insert(());
        let target = package.build_target(TargetKind::Source);
        let artifact = ArtifactKey::BuildMi {
            package,
            target_kind: TargetKind::Source,
        };

        let mut plan = ArtifactPlan::default();
        plan.provide(BuildPlanNode::BuildCore(target), artifact);
        plan.provide(BuildPlanNode::Check(target), artifact);
    }
}
