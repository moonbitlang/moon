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

mod cmd_test;
mod test_cases;
mod util;

use std::path::{Path, PathBuf};
use util::*;

struct TestDir {
    // tempfile::TempDir has a drop implementation that will remove the directory
    // copy the test directory to a temporary directory to abvoid conflict with other tests when `cargo test` parallelly testing
    path: tempfile::TempDir,
}

impl TestDir {
    // create a new TestDir with the test directory in tests/test_cases/<sub>
    fn new(sub: &str) -> Self {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/test_cases")
            .join(sub);
        let tmp_dir = tempfile::TempDir::new().unwrap();
        copy(&dir, tmp_dir.path()).unwrap();
        Self { path: tmp_dir }
    }

    // create a empty TestDir
    fn new_empty() -> Self {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        Self { path: tmp_dir }
    }

    fn join(&self, sub: impl AsRef<str>) -> PathBuf {
        self.path.path().join(sub.as_ref())
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        self.path.path()
    }
}

#[track_caller]
fn get_stdout_without_replace(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(dir)
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();

    let s = std::str::from_utf8(&out).unwrap().to_string();
    s
}

#[track_caller]
fn get_stderr_without_replace(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(dir)
        .args(args)
        .assert()
        .success()
        .get_output()
        .stderr
        .to_owned();

    let s = std::str::from_utf8(&out).unwrap().to_string();
    s
}

fn get_err_stdout_without_replace(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(dir)
        .args(args)
        .assert()
        .failure()
        .get_output()
        .stdout
        .to_owned();

    let s = std::str::from_utf8(&out).unwrap().to_string();
    s
}

#[track_caller]
fn get_err_stderr_without_replace(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let out = snapbox::cmd::Command::new(moon_bin())
        .current_dir(dir)
        .args(args)
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();

    let s = std::str::from_utf8(&out).unwrap().to_string();
    s
}

#[track_caller]
pub fn get_stdout(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_stdout_without_replace(dir, args);
    replace_dir(&s, dir)
}

#[track_caller]
pub fn get_stderr(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_stderr_without_replace(dir, args);
    replace_dir(&s, dir)
}

#[track_caller]
pub fn get_err_stdout(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_err_stdout_without_replace(dir, args);
    replace_dir(&s, dir)
}

#[track_caller]
pub fn get_err_stderr(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_err_stderr_without_replace(dir, args);
    replace_dir(&s, dir)
}
