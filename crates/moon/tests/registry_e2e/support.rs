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
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use moonbuild_debug::graph::ENV_VAR;

static MOONRUN_BIN: OnceLock<PathBuf> = OnceLock::new();

pub(crate) struct TestDir(moon_test_util::test_dir::TestDir);

impl TestDir {
    pub(crate) fn new(case: &str) -> Self {
        let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
        Self(moon_test_util::test_dir::TestDir::from_case_root(
            case_root, case, true,
        ))
    }

    pub(crate) fn new_empty() -> Self {
        Self(moon_test_util::test_dir::TestDir::new_empty())
    }

    pub(crate) fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.0.join(path)
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}

pub(crate) fn moon_bin() -> PathBuf {
    snapbox::cargo_bin!("moon").to_owned()
}

fn moonrun_bin() -> PathBuf {
    MOONRUN_BIN
        .get_or_init(|| {
            escargot::CargoBuild::new()
                .manifest_path(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../moonrun/Cargo.toml"),
                )
                .bin("moonrun")
                .current_release()
                .current_target()
                .run()
                .expect("failed to build moonrun")
                .path()
                .to_owned()
        })
        .clone()
}

fn toolchain_root_for_tests() -> PathBuf {
    if let Some(path) = std::env::var_os("MOON_TOOLCHAIN_ROOT") {
        return PathBuf::from(path);
    }

    let moonc = dunce::canonicalize(&*moonutil::toolchain::BINARIES.moonc)
        .unwrap_or(moonutil::toolchain::BINARIES.moonc.clone());
    if let Some(bin_dir) = moonc.parent()
        && bin_dir.file_name().is_some_and(|name| name == "bin")
        && let Some(root) = bin_dir.parent()
        && moonutil::toolchain::is_toolchain_root(root)
    {
        return root.to_path_buf();
    }

    moonutil::toolchain::toolchain_root()
}

pub(crate) fn moon_cmd(dir: &impl AsRef<Path>) -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(moon_bin())
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .env("MOON_DEP_CACHE", "off")
        .current_dir(dir)
}

pub(crate) fn get_stdout_with_envs(
    dir: &impl AsRef<Path>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
) -> String {
    let assert = moon_cmd(dir).envs(envs).args(args).assert().success();
    String::from_utf8(assert.get_output().stdout.clone()).expect("moon stdout should be UTF-8")
}

pub(crate) fn snap_dry_run_graph_with_envs(
    dir: &impl AsRef<Path>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    to_file: &impl AsRef<Path>,
    envs: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
) {
    get_stdout_with_envs(
        dir,
        args,
        std::iter::once((
            OsString::from(ENV_VAR),
            to_file.as_ref().as_os_str().to_owned(),
        ))
        .chain(
            envs.into_iter()
                .map(|(name, value)| (name.as_ref().to_owned(), value.as_ref().to_owned())),
        ),
    );
}

pub(crate) fn path_with_moon() -> OsString {
    std::env::join_paths(
        std::iter::once(
            moon_bin()
                .parent()
                .expect("test moon binary should have a parent directory")
                .to_path_buf(),
        )
        .chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
    )
    .expect("test PATH should be valid")
}
