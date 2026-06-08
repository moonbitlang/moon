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

use moonutil::mooncakes::ModuleId;

use crate::model::{BuildTarget, PackageId};

use super::BuildActionId;

/// A logical artifact produced by a build action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlannedArtifact {
    PackageInterface {
        producer: BuildActionId,
        target: BuildTarget,
    },
    PackageCoreIr {
        producer: BuildActionId,
        target: BuildTarget,
    },
    ProofInterface {
        producer: BuildActionId,
        target: BuildTarget,
    },
    ProofWhyml {
        producer: BuildActionId,
        target: BuildTarget,
    },
    ProofReport {
        producer: BuildActionId,
        target: BuildTarget,
    },
    CStubObject {
        producer: BuildActionId,
        package: PackageId,
        index: u32,
    },
    CStubLibrary {
        producer: BuildActionId,
        package: PackageId,
    },
    LinkedCore {
        producer: BuildActionId,
        target: BuildTarget,
    },
    Executable {
        producer: BuildActionId,
        target: BuildTarget,
    },
    GeneratedTestDriver {
        producer: BuildActionId,
        target: BuildTarget,
    },
    GeneratedTestMetadata {
        producer: BuildActionId,
        target: BuildTarget,
    },
    BundleResult {
        producer: BuildActionId,
        module: ModuleId,
    },
    RuntimeLib {
        producer: BuildActionId,
    },
    GeneratedMbti {
        producer: BuildActionId,
        target: BuildTarget,
    },
    DocsDir {
        producer: BuildActionId,
    },
    VirtualPackageInterface {
        producer: BuildActionId,
        package: PackageId,
    },
    MoonLexGeneratedSource {
        producer: BuildActionId,
        package: PackageId,
        index: u32,
    },
    MoonYaccGeneratedSource {
        producer: BuildActionId,
        package: PackageId,
        index: u32,
    },
    KnownPath {
        producer: BuildActionId,
        path: PathBuf,
    },
}

impl PlannedArtifact {
    pub fn producer(&self) -> BuildActionId {
        match self {
            PlannedArtifact::PackageInterface { producer, .. }
            | PlannedArtifact::PackageCoreIr { producer, .. }
            | PlannedArtifact::ProofInterface { producer, .. }
            | PlannedArtifact::ProofWhyml { producer, .. }
            | PlannedArtifact::ProofReport { producer, .. }
            | PlannedArtifact::CStubObject { producer, .. }
            | PlannedArtifact::CStubLibrary { producer, .. }
            | PlannedArtifact::LinkedCore { producer, .. }
            | PlannedArtifact::Executable { producer, .. }
            | PlannedArtifact::GeneratedTestDriver { producer, .. }
            | PlannedArtifact::GeneratedTestMetadata { producer, .. }
            | PlannedArtifact::BundleResult { producer, .. }
            | PlannedArtifact::RuntimeLib { producer }
            | PlannedArtifact::GeneratedMbti { producer, .. }
            | PlannedArtifact::DocsDir { producer }
            | PlannedArtifact::VirtualPackageInterface { producer, .. }
            | PlannedArtifact::MoonLexGeneratedSource { producer, .. }
            | PlannedArtifact::MoonYaccGeneratedSource { producer, .. }
            | PlannedArtifact::KnownPath { producer, .. } => *producer,
        }
    }
}
