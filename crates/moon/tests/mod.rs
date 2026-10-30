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

mod support;
mod test_cases;

use std::path::{Path, PathBuf};
use util::*;

pub(crate) use support::{build_graph, dry_run_utils, process, util};

pub(crate) struct TestDir(moon_test_util::test_dir::TestDir);

impl TestDir {
    // create a new TestDir with the test directory in tests/test_cases/<sub>
    fn new(sub: &str) -> Self {
        let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
        Self(moon_test_util::test_dir::TestDir::from_case_root(
            case_root, sub, true,
        ))
    }

    // create a empty TestDir
    fn new_empty() -> Self {
        Self(moon_test_util::test_dir::TestDir::new_empty())
    }

    fn join(&self, sub: impl AsRef<str>) -> PathBuf {
        self.0.join(sub.as_ref())
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}

fn packages_selector_path(dir: impl AsRef<Path>) -> PathBuf {
    target_packages_selector_path(dir.as_ref().join("_build"))
}

fn packages_index_path(dir: impl AsRef<Path>) -> PathBuf {
    target_packages_index_path(dir.as_ref().join("_build"))
}

fn scoped_packages_json_path(dir: impl AsRef<Path>, backend: &str, profile: &str) -> PathBuf {
    target_scoped_packages_json_path(dir.as_ref().join("_build"), backend, profile)
}

fn target_packages_selector_path(target_dir: impl AsRef<Path>) -> PathBuf {
    target_dir.as_ref().join("packages.json")
}

fn target_packages_index_path(target_dir: impl AsRef<Path>) -> PathBuf {
    target_dir.as_ref().join("index.json")
}

fn target_scoped_packages_json_path(
    target_dir: impl AsRef<Path>,
    backend: &str,
    profile: &str,
) -> PathBuf {
    target_dir
        .as_ref()
        .join(format!("{backend}/{profile}/check/packages.json"))
}

fn standalone_target_dir(dir: impl AsRef<Path>, source_filename: &str) -> PathBuf {
    dir.as_ref().join("_build").join(source_filename)
}

pub fn moon_cmd(dir: &impl AsRef<Path>) -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(moon_bin())
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .env("MOON_DEP_CACHE", "off")
        .current_dir(dir)
}

pub fn moon_process_cmd(dir: &impl AsRef<Path>) -> std::process::Command {
    let mut cmd = std::process::Command::new(moon_bin());
    cmd.env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .env("MOON_DEP_CACHE", "off")
        .current_dir(dir);
    cmd
}

enum ExpectedStatus {
    Success,
    Failure,
}

enum OutputStream {
    Stdout,
    Stderr,
}

#[track_caller]
fn get_output_without_replace(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<std::ffi::OsStr>, impl AsRef<std::ffi::OsStr>)>,
    status: ExpectedStatus,
    stream: OutputStream,
) -> String {
    let assert = moon_cmd(dir).envs(envs).args(args).assert();
    let assert = match status {
        ExpectedStatus::Success => assert.success(),
        ExpectedStatus::Failure => assert.failure(),
    };
    let output = assert.get_output();
    let out = match stream {
        OutputStream::Stdout => &output.stdout,
        OutputStream::Stderr => &output.stderr,
    };

    std::str::from_utf8(out).unwrap().to_string()
}

#[track_caller]
pub fn get_stdout(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_output_without_replace(
        dir,
        args,
        [] as [(&str, &str); 0],
        ExpectedStatus::Success,
        OutputStream::Stdout,
    );
    replace_dir(&s, dir)
}

#[track_caller]
pub fn get_stdout_with_envs(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<std::ffi::OsStr>, impl AsRef<std::ffi::OsStr>)>,
) -> String {
    let s = get_output_without_replace(
        dir,
        args,
        envs,
        ExpectedStatus::Success,
        OutputStream::Stdout,
    );
    replace_dir(&s, dir)
}

#[track_caller]
pub fn get_stderr(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_output_without_replace(
        dir,
        args,
        [] as [(&str, &str); 0],
        ExpectedStatus::Success,
        OutputStream::Stderr,
    );
    replace_dir(&s, dir)
}

#[track_caller]
pub fn get_stderr_with_envs(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<std::ffi::OsStr>, impl AsRef<std::ffi::OsStr>)>,
) -> String {
    let s = get_output_without_replace(
        dir,
        args,
        envs,
        ExpectedStatus::Success,
        OutputStream::Stderr,
    );
    replace_dir(&s, dir)
}

#[track_caller]
pub fn assert_success(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) {
    moon_cmd(dir).args(args).assert().success();
}

#[track_caller]
pub fn get_err_stdout(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_output_without_replace(
        dir,
        args,
        [] as [(&str, &str); 0],
        ExpectedStatus::Failure,
        OutputStream::Stdout,
    );
    replace_dir(&s, dir)
}

#[track_caller]
pub fn get_err_stderr(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_output_without_replace(
        dir,
        args,
        [] as [(&str, &str); 0],
        ExpectedStatus::Failure,
        OutputStream::Stderr,
    );
    replace_dir(&s, dir)
}

#[track_caller]
pub fn get_err_stderr_with_envs(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<std::ffi::OsStr>, impl AsRef<std::ffi::OsStr>)>,
) -> String {
    let s = get_output_without_replace(
        dir,
        args,
        envs,
        ExpectedStatus::Failure,
        OutputStream::Stderr,
    );
    replace_dir(&s, dir)
}
