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

//! Generation of `all_pkgs.json` metadata file.
//!
//! This module provides functionality to generate the `all_pkgs.json` file,
//! which is a metadata file used by `moonc` to resolve indirect dependencies.
//! The file contains information about all packages in the build, including
//! their root modules, relative paths, and artifact locations (`.mi` files).
//!
//! The `all_pkgs.json` file is generated in the target directory during the
//! build process and is consumed by the MoonBit compiler for dependency
//! analysis.

use moonutil::common::TargetBackend;
use serde::{Deserialize, Serialize};

use crate::{ResolveOutput, model::TargetKind};

pub const ALL_PKGS_JSON: &str = "all_pkgs.json";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct PackageArtifactJSON {
    root: String,
    rel: String,
    artifact: String,
}

/// The build system generates a `all_pkgs.json` file in the target directory
/// which contains all the packages with their `mi`` files (the artifact). This
/// is used by `moonc` to analyze the indirect dependency.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct AllPkgsJSON {
    packages: Vec<PackageArtifactJSON>,
}

/// Generate `all_pkgs.json`, which is a metadata file for resolving indirect
/// dependencies.
pub fn gen_all_pkgs_json(
    resolve_output: &ResolveOutput,
    layout: &crate::build_lower::artifact::LegacyLayout,
    backend: TargetBackend,
) -> AllPkgsJSON {
    let mut packages: Vec<PackageArtifactJSON> = resolve_output
        .pkg_dirs
        .all_packages()
        // Skip the `moonbitlang/core/abort` package to match the behavior of the legacy metadata JSON
        .filter(|(id, _)| {
            resolve_output
                .pkg_dirs
                .abort_pkg()
                .is_none_or(|id2| *id != id2)
        })
        .map(|(id, _)| {
            let pkg = resolve_output.pkg_dirs.get_package(id);
            let root = pkg.fqn.module().name().to_string();
            let rel = pkg.fqn.package().to_string();
            let artifact = layout
                .mi_of_build_target(
                    &resolve_output.pkg_dirs,
                    &id.build_target(TargetKind::Source),
                    backend,
                )
                .to_string_lossy()
                .into_owned();
            PackageArtifactJSON {
                root,
                rel,
                artifact,
            }
        })
        .collect();
    packages.sort_by(|x, y| (x.root.cmp(&y.root)).then_with(|| x.rel.cmp(&y.rel)));

    AllPkgsJSON { packages }
}
