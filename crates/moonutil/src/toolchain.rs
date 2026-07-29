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

//! MoonBit toolchain layout and executable resolution.
//!
//! This module groups facts about the installed MoonBit toolchain: its root,
//! shipped `bin`/`lib`/`include` directories, shipped standard-library
//! artifacts, and resolved tool executable paths. Project-local build layout
//! should live outside this module.

use anyhow::Context;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

pub use crate::binaries::{BINARIES, CachedBinaries};
pub use crate::moon_dir::{
    MOON_DIRS, MoonDirs, RESERVED_BIN_NAMES, abort_core_in, abort_mi_in, bin, core, core_bundle,
    core_bundle_in, core_core, core_core_in, core_package_mi_in, home, include, is_toolchain_root,
    lib, toolchain_root, user_bin, why3_datadir, why3_libdir,
};

/// Return the runtime C translation units shipped by the selected toolchain.
///
/// New toolchains split the runtime across `lib/runtime/*.c`. Keep the legacy
/// single-file layout as a fallback during the toolchain transition.
pub fn runtime_source_paths() -> anyhow::Result<Vec<PathBuf>> {
    runtime_source_paths_in(&lib())
}

fn runtime_source_paths_in(lib_path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let runtime_dir = lib_path.join("runtime");
    if runtime_dir.is_dir() {
        let mut sources = fs::read_dir(&runtime_dir)
            .with_context(|| {
                format!(
                    "failed to read runtime source directory {}",
                    runtime_dir.display()
                )
            })?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| {
                format!(
                    "failed to read runtime source directory {}",
                    runtime_dir.display()
                )
            })?;
        sources.retain(|path| path.extension() == Some(OsStr::new("c")));
        sources.sort();
        if !sources.is_empty() {
            return Ok(sources);
        }
    }

    let legacy_runtime = lib_path.join("runtime.c");
    if legacy_runtime.is_file() {
        return Ok(vec![legacy_runtime]);
    }

    anyhow::bail!(
        "no runtime C sources found in {} or at {}",
        runtime_dir.display(),
        legacy_runtime.display()
    )
}

/// Resolves an executable using the platform's command lookup rules.
///
/// `which` may return a relative path when `PATH` contains relative entries.
/// Build actions can run from a different working directory, so executable
/// paths crossing the toolchain boundary must be made absolute here.
pub(crate) fn resolve_executable(tool: impl AsRef<OsStr>) -> anyhow::Result<PathBuf> {
    let tool = tool.as_ref();
    resolve_executable_with(tool, |tool| which::which(tool))
}

fn resolve_executable_with(
    tool: &OsStr,
    find: impl FnOnce(&OsStr) -> Result<PathBuf, which::Error>,
) -> anyhow::Result<PathBuf> {
    let resolved = find(tool)
        .with_context(|| format!("failed to find executable `{}`", Path::new(tool).display()))?;
    std::path::absolute(&resolved).with_context(|| {
        format!(
            "failed to make executable path `{}` absolute",
            resolved.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{resolve_executable_with, runtime_source_paths_in};

    #[test]
    fn runtime_sources_use_sorted_split_runtime_files() {
        let dir = tempfile::tempdir().unwrap();
        let runtime_dir = dir.path().join("runtime");
        std::fs::create_dir(&runtime_dir).unwrap();
        std::fs::write(runtime_dir.join("z.c"), []).unwrap();
        std::fs::write(runtime_dir.join("a.c"), []).unwrap();
        std::fs::write(runtime_dir.join("README.md"), []).unwrap();
        std::fs::write(dir.path().join("runtime.c"), []).unwrap();

        assert_eq!(
            runtime_source_paths_in(dir.path()).unwrap(),
            vec![runtime_dir.join("a.c"), runtime_dir.join("z.c")]
        );
    }

    #[test]
    fn runtime_sources_fall_back_to_legacy_runtime_file() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_runtime = dir.path().join("runtime.c");
        std::fs::write(&legacy_runtime, []).unwrap();

        assert_eq!(
            runtime_source_paths_in(dir.path()).unwrap(),
            vec![legacy_runtime]
        );
    }

    fn test_executable_name(stem: &str) -> String {
        if cfg!(windows) {
            format!("{stem}.exe")
        } else {
            stem.to_string()
        }
    }

    #[test]
    fn executable_from_relative_search_path_is_made_absolute() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/native-toolchain/bin");
        let current_dir = std::env::current_dir().expect("get test current directory");
        let relative_fixture = fixture
            .strip_prefix(&current_dir)
            .expect("fixture should be below the test current directory");
        let executable_name = test_executable_name("fake-gcc");

        let resolved = resolve_executable_with(executable_name.as_ref(), |tool| {
            which::which_in(tool, Some(relative_fixture), &current_dir)
        })
        .expect("resolve fixture executable");

        assert_eq!(resolved, fixture.join(executable_name));
        assert!(resolved.is_absolute());
    }

    #[test]
    fn executable_from_explicit_relative_path_is_made_absolute() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/native-toolchain/bin");
        let current_dir = std::env::current_dir().expect("get test current directory");
        let executable = fixture.join(test_executable_name("fake-gcc"));
        let relative_executable = executable
            .strip_prefix(&current_dir)
            .expect("fixture should be below the test current directory");

        let resolved = super::resolve_executable(relative_executable)
            .expect("resolve explicit relative executable path");

        assert_eq!(resolved, executable);
        assert!(resolved.is_absolute());
    }

    #[cfg(windows)]
    #[test]
    fn windows_command_script_is_resolved_through_pathext() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/native-toolchain/bin");
        let current_dir = std::env::current_dir().expect("get test current directory");
        let relative_fixture = fixture
            .strip_prefix(&current_dir)
            .expect("fixture should be below the test current directory");

        let resolved = resolve_executable_with("fake-script".as_ref(), |tool| {
            which::which_in(tool, Some(relative_fixture), &current_dir)
        })
        .expect("resolve command script through PATHEXT");

        assert_eq!(resolved, fixture.join("fake-script.cmd"));
        assert!(resolved.is_absolute());
    }
}
