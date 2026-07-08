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

use moonutil::resolution::ModuleId;

use crate::model::{BuildTarget, PackageId};

/// A logical product selected from a build node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildProduct {
    PackageInterface {
        target: BuildTarget,
    },
    PackageCoreIr {
        target: BuildTarget,
    },
    ProofInterface {
        target: BuildTarget,
    },
    ProofWhyml {
        target: BuildTarget,
    },
    ProofReport {
        target: BuildTarget,
    },
    CStubObject {
        package: PackageId,
        index: u32,
    },
    CStubLibrary {
        package: PackageId,
    },
    LinkedCore {
        target: BuildTarget,
    },
    Executable {
        target: BuildTarget,
    },
    GeneratedTestDriver {
        target: BuildTarget,
    },
    GeneratedTestMetadata {
        target: BuildTarget,
    },
    BundleResult {
        module: ModuleId,
    },
    RuntimeLib,
    GeneratedMbti {
        target: BuildTarget,
    },
    DocsDir,
    VirtualPackageInterface {
        package: PackageId,
    },
    MoonLexGeneratedSource {
        package: PackageId,
        index: u32,
    },
    MoonYaccGeneratedSource {
        package: PackageId,
        index: u32,
    },
    /// Prebuild commands already resolve output paths while planning.
    PrebuildOutputPath {
        path: PathBuf,
    },
}
