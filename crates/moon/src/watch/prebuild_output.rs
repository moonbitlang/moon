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

//! Get the prebuild paths that need special handling during watch mode.

use std::path::{Path, PathBuf};

use moonbuild_rupes_recta::ResolveOutput;
use moonutil::package::MoonPkgGenerate;

pub(crate) struct PrebuildWatchPaths {
    pub ignored_paths: Vec<PathBuf>,
    pub watched_paths: Vec<PathBuf>,
}

/// Generate the list of paths to watch or ignore for pre-builds during watch mode.
pub(crate) fn rr_get_prebuild_watch_paths(env: &ResolveOutput) -> PrebuildWatchPaths {
    let mut ignored_paths = vec![];
    let mut watched_paths = vec![];

    for &m in env.local_modules() {
        let Some(packages) = env.pkg_dirs.packages_for_module(m) else {
            continue;
        };
        for &pkg_id in packages.values() {
            let pkg = env.pkg_dirs.get_package(pkg_id);
            if let Some(prebuild) = pkg.raw.pre_build.as_ref() {
                push_prebuild_paths(
                    &mut ignored_paths,
                    &mut watched_paths,
                    prebuild,
                    &pkg.root_path,
                );
            }
        }
    }

    PrebuildWatchPaths {
        ignored_paths,
        watched_paths,
    }
}

fn push_prebuild_paths(
    ignored_paths: &mut Vec<PathBuf>,
    watched_paths: &mut Vec<PathBuf>,
    pre_build: &[MoonPkgGenerate],
    pkg_root: &Path,
) {
    for v in pre_build {
        for i in v.input.iter() {
            watched_paths.push(pkg_root.join(i));
        }
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
    use moonutil::dirs::PackageDirs;

    #[test]
    fn rr_get_prebuild_watch_paths_skips_empty_modules() {
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
            &PackageDirs::mooncakes_dir_for_source(temp_dir.path()),
        )
        .unwrap();

        let watch_paths = rr_get_prebuild_watch_paths(&resolved);
        assert!(watch_paths.ignored_paths.is_empty());
        assert!(watch_paths.watched_paths.is_empty());
    }

    #[test]
    fn rr_get_prebuild_watch_paths_collects_inputs_and_outputs() {
        use std::fs;

        let temp_dir = tempfile::tempdir().unwrap();
        fs::write(
            temp_dir.path().join("moon.mod.json"),
            r#"{"name":"user/prebuild"}"#,
        )
        .unwrap();
        fs::write(
            temp_dir.path().join("moon.pkg.json"),
            r#"{
                "pre-build": [
                    {
                        "input": ["assets/a.txt", "assets/b.txt"],
                        "output": ["generated/a.mbt", "generated/b.txt"],
                        "command": "tool"
                    }
                ]
            }"#,
        )
        .unwrap();

        let resolved = resolve(
            &ResolveConfig::new_with_load_defaults(false, false, false),
            temp_dir.path(),
            &PackageDirs::mooncakes_dir_for_source(temp_dir.path()),
        )
        .unwrap();

        let watch_paths = rr_get_prebuild_watch_paths(&resolved);
        let root = dunce::canonicalize(temp_dir.path()).unwrap();
        assert_eq!(
            watch_paths.watched_paths,
            vec![root.join("assets/a.txt"), root.join("assets/b.txt"),]
        );
        assert_eq!(
            watch_paths.ignored_paths,
            vec![root.join("generated/a.mbt"), root.join("generated/b.txt"),]
        );
    }
}
