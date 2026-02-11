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

//! Get the prebuild outputs that needs to be ignored during watch mode.

use std::path::{Path, PathBuf};

use moonbuild_rupes_recta::ResolveOutput;
use moonutil::package::MoonPkgGenerate;

/// Generate the list of paths to ignore from pre-build outputs for
/// [`super::WatchOutput`], in RR backend.
pub(crate) fn rr_get_prebuild_ignored_paths(env: &ResolveOutput) -> Vec<PathBuf> {
    let mut ignored_paths = vec![];

    for &m in env.local_modules() {
        for &pkg_id in env
            .pkg_dirs
            .packages_for_module(m)
            .expect("Module should exist")
            .values()
        {
            let pkg = env.pkg_dirs.get_package(pkg_id);
            if let Some(prebuild) = pkg.raw.pre_build.as_ref() {
                push_prebuild_paths(&mut ignored_paths, prebuild, &pkg.root_path);
            }
        }
    }

    ignored_paths
}

fn push_prebuild_paths(
    ignored_paths: &mut Vec<PathBuf>,
    pre_build: &[MoonPkgGenerate],
    pkg_root: &Path,
) {
    for v in pre_build {
        for o in v.output.iter() {
            let path = pkg_root.join(o);
            ignored_paths.push(path);
        }
    }
}
