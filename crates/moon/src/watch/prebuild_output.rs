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
        let Some(packages) = env.pkg_dirs.packages_for_module(m) else {
            continue;
        };
        for &pkg_id in packages.values() {
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

#[cfg(test)]
mod tests {
    use super::*;

    use moonbuild_rupes_recta::resolve::{ResolveConfig, resolve};

    #[test]
    fn rr_get_prebuild_ignored_paths_skips_empty_modules() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        fs::write(
            temp_dir.path().join("moon.mod.json"),
            r#"{"name":"user/empty"}"#,
        )
        .unwrap();

        let resolved = resolve(
            &ResolveConfig::new_with_load_defaults(false, false, false),
            temp_dir.path(),
        )
        .unwrap();

        assert!(rr_get_prebuild_ignored_paths(&resolved).is_empty());
    }
}
