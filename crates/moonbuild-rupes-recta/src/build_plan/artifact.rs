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

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use indexmap::IndexSet;
use moonutil::resolution::ModuleId;

use crate::model::{BuildPlanNode, BuildTarget, PackageId, TargetKind};

/// Normalize a package file declaration without retaining the package's
/// physical root. Explicit paths outside the package remain absolute because
/// the declaration itself has no package-relative identity.
pub(crate) fn package_file_key(package_root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(package_root)
        .unwrap_or(path)
        .to_path_buf()
}

/// Identify a runtime translation unit within the stable toolchain layout.
pub(crate) fn runtime_source_key(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .expect("runtime source should have a file name");
    if path.parent().and_then(Path::file_name) == Some("runtime".as_ref()) {
        Path::new("runtime").join(file_name)
    } else {
        PathBuf::from(file_name)
    }
}

/// The logical identity of one build result within one configuration-scoped
/// build plan.
///
/// Physical output roots and provider actions are intentionally absent.
/// Check, build, and virtual-contract interfaces are distinct because they are
/// not interchangeable inputs even though all three currently use `.mi` files.
/// Backend, profile, and run-mode scope live on the plan until one plan is able
/// to contain more than one such configuration.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ArtifactKey {
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
    ProofMi {
        package: PackageId,
        target_kind: TargetKind,
    },
    ProofWhyml {
        package: PackageId,
        target_kind: TargetKind,
    },
    ProofReport {
        package: PackageId,
        target_kind: TargetKind,
    },
    CStubObject {
        package: PackageId,
        /// Normalized declaration path, relative to the package root.
        source: PathBuf,
    },
    CStubLibrary {
        package: PackageId,
    },
    LinkedCore {
        package: PackageId,
        target_kind: TargetKind,
    },
    Executable {
        package: PackageId,
        target_kind: TargetKind,
    },
    GeneratedTestDriver {
        package: PackageId,
        target_kind: TargetKind,
    },
    GeneratedTestMetadata {
        package: PackageId,
        target_kind: TargetKind,
    },
    BundleResult {
        module: ModuleId,
    },
    RuntimeObject {
        /// Path within the toolchain library layout (`runtime/foo.c` or
        /// legacy `runtime.c`), independent of the installed toolchain root.
        source: PathBuf,
    },
    RuntimeLibrary,
    GeneratedMbti {
        package: PackageId,
        target_kind: TargetKind,
    },
    DocsDir {
        module: ModuleId,
    },
    PrebuildOutput {
        package: PackageId,
        /// Normalized declaration path. Supported outputs are relative to the
        /// package root; an explicitly absolute declaration remains absolute.
        path: PathBuf,
    },
}

impl ArtifactKey {
    /// The package target whose compilation lifecycle owns this artifact.
    /// Module-wide, package-wide, runtime, and prebuild artifacts have no
    /// `BuildTarget` identity.
    pub(crate) fn package_target(&self) -> Option<BuildTarget> {
        match self {
            Self::CheckMi {
                package,
                target_kind,
            }
            | Self::BuildMi {
                package,
                target_kind,
            }
            | Self::CoreIr {
                package,
                target_kind,
            }
            | Self::ProofMi {
                package,
                target_kind,
            }
            | Self::ProofWhyml {
                package,
                target_kind,
            }
            | Self::ProofReport {
                package,
                target_kind,
            }
            | Self::LinkedCore {
                package,
                target_kind,
            }
            | Self::Executable {
                package,
                target_kind,
            }
            | Self::GeneratedTestDriver {
                package,
                target_kind,
            }
            | Self::GeneratedTestMetadata {
                package,
                target_kind,
            }
            | Self::GeneratedMbti {
                package,
                target_kind,
            } => Some(package.build_target(*target_kind)),
            Self::VirtualContractMi { .. }
            | Self::CStubObject { .. }
            | Self::CStubLibrary { .. }
            | Self::BundleResult { .. }
            | Self::RuntimeObject { .. }
            | Self::RuntimeLibrary
            | Self::DocsDir { .. }
            | Self::PrebuildOutput { .. } => None,
        }
    }
}

/// Artifact providers and the artifact requirements of each action.
///
/// The existing high-level [`BuildPlanNode`] identifies an action. Multiple
/// artifacts may name the same provider without introducing a positional or
/// arena ID.
#[derive(Debug, Default)]
pub(super) struct ArtifactPlan {
    providers: HashMap<ArtifactKey, BuildPlanNode>,
    artifacts_by_provider: HashMap<BuildPlanNode, IndexSet<ArtifactKey>>,
    requirements_by_consumer: HashMap<BuildPlanNode, IndexSet<ArtifactKey>>,
}

impl ArtifactPlan {
    pub(super) fn require(&mut self, consumer: BuildPlanNode, artifact: ArtifactKey) {
        self.requirements_by_consumer
            .entry(consumer)
            .or_default()
            .insert(artifact);
    }

    pub(super) fn provide(&mut self, provider: BuildPlanNode, artifact: ArtifactKey) {
        if let Some(&existing) = self.providers.get(&artifact) {
            assert_eq!(
                existing, provider,
                "artifact {artifact:?} has conflicting providers {existing:?} and {provider:?}"
            );
        } else {
            self.providers.insert(artifact.clone(), provider);
        }
        self.artifacts_by_provider
            .entry(provider)
            .or_default()
            .insert(artifact);
    }

    pub(super) fn provider(&self, artifact: &ArtifactKey) -> Option<BuildPlanNode> {
        self.providers.get(artifact).copied()
    }

    pub(super) fn requirements(&self) -> impl Iterator<Item = (BuildPlanNode, ArtifactKey)> + '_ {
        self.requirements_by_consumer
            .iter()
            .flat_map(|(&consumer, requirements)| {
                requirements
                    .iter()
                    .cloned()
                    .map(move |artifact| (consumer, artifact))
            })
    }

    pub(super) fn provided_by(
        &self,
        provider: BuildPlanNode,
    ) -> impl Iterator<Item = &ArtifactKey> {
        self.artifacts_by_provider
            .get(&provider)
            .into_iter()
            .flat_map(IndexSet::iter)
    }

    pub(super) fn validate(&self) {
        for (_, artifact) in self.requirements() {
            assert!(
                self.providers.contains_key(&artifact),
                "required artifact {artifact:?} has no provider in the build plan"
            );
        }
    }

    pub(super) fn dependencies(
        &self,
        consumer: BuildPlanNode,
    ) -> impl Iterator<Item = (BuildPlanNode, ArtifactKey)> + '_ {
        self.requirements_by_consumer
            .get(&consumer)
            .into_iter()
            .flat_map(|requirements| requirements.iter().cloned())
            .map(|artifact| {
                let provider = self.providers[&artifact];
                (provider, artifact)
            })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use slotmap::SlotMap;

    use super::*;

    #[test]
    fn multiple_artifacts_can_share_one_provider() {
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
        plan.provide(provider, build_mi.clone());
        plan.provide(provider, core_ir.clone());

        assert_eq!(plan.provider(&build_mi), Some(provider));
        assert_eq!(plan.provider(&core_ir), Some(provider));
        assert_eq!(plan.providers.len(), 2);
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
        plan.require(consumer, build_mi.clone());

        assert_eq!(
            plan.requirements().collect::<HashSet<_>>(),
            HashSet::from([(consumer, build_mi.clone())])
        );
        assert_eq!(plan.provider(&build_mi), None);
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
        plan.provide(BuildPlanNode::BuildCore(target), artifact.clone());
        plan.provide(BuildPlanNode::Check(target), artifact);
    }
}
