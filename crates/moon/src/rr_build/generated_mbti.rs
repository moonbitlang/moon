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

//! Locating generated package interface (`.mbti`) files.
//!
//! A build generates `.mbti` files under the target directory. Commands that
//! publish them copy the files to [`MBTI_GENERATED`] in the package source
//! directory.

use std::path::{Path, PathBuf};

use moonbuild::BuildMeta;
use moonbuild_rupes_recta::{DiscoveredProject, build_plan::ArtifactKey, model::PackageId};
use moonutil::constants::MBTI_GENERATED;

/// The source-tree path that a package's generated interface is published to.
pub(crate) fn generated_mbti_dest(discovered: &DiscoveredProject, package: PackageId) -> PathBuf {
    discovered
        .pkg_dirs
        .get_package(package)
        .root_path
        .join(MBTI_GENERATED)
}

/// The generated `.mbti` artifacts planned by a build, with their packages.
pub(crate) fn generated_mbti_artifacts(
    meta: &BuildMeta,
) -> impl Iterator<Item = (PackageId, &Path)> {
    meta.artifacts.iter().filter_map(|(artifact, paths)| {
        let ArtifactKey::GeneratedMbti {
            package,
            target_kind: _,
        } = artifact
        else {
            return None;
        };
        let [path] = paths.as_slice() else {
            panic!("mbti generation should only produce one artifact");
        };
        Some((*package, path.as_path()))
    })
}
