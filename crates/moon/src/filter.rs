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

//! Path filtering operations
//!
//! This module contains the common path filtering logic for both legacy and RR backends.

use std::path::{Path, PathBuf};

use anyhow::Context;
use moonbuild_rupes_recta::{ResolveOutput, model::PackageId};

/// Canonicalize the given path, returning the directory it's referencing, and
/// an optional filename if the path is a file.
pub fn canonicalize_with_filename(path: &Path) -> anyhow::Result<(PathBuf, Option<String>)> {
    let input_path = dunce::canonicalize(path).with_context(|| {
        format!(
            "Failed to canonicalize input filter directory `{}`",
            path.display()
        )
    })?;
    if input_path.is_dir() {
        Ok((input_path, None))
    } else {
        let filename = input_path
            .file_name()
            .with_context(|| {
                format!(
                    "Failed to get filename from input filter path `{}`",
                    input_path.display()
                )
            })?
            .to_str()
            .with_context(|| {
                format!(
                    "Input filename is not valid UTF-8: {}",
                    input_path.display()
                )
            })?
            .to_owned();

        let mut parent = input_path;
        parent.pop();

        Ok((parent.to_path_buf(), Some(filename)))
    }
}

/// From a canonicalized, directory path, find the corresponding package ID.
pub fn filter_pkg_by_dir(resolve_output: &ResolveOutput, dir: &Path) -> Option<PackageId> {
    let mut all_local_packages = resolve_output.local_modules().iter().flat_map(|&it| {
        resolve_output
            .pkg_dirs
            .packages_for_module(it)
            .unwrap()
            .values()
            .cloned()
    });

    all_local_packages.find(|&pkg_id| {
        let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
        pkg.root_path == dir
    })
}
