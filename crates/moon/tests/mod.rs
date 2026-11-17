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

const TEST_CASES_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test_cases");

pub(crate) struct TestDir(moon_test_util::test_dir::TestDir);

impl TestDir {
    // create a new TestDir with the test directory in tests/test_cases/<sub>
    fn new(sub: &str) -> Self {
        Self(moon_test_util::test_dir::TestDir::from_case_root(
            TEST_CASES_ROOT,
            sub,
            true,
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

pub(crate) struct Fixture {
    cwd: PathBuf,
    target_dir: Option<PathBuf>,
    // Owns the temporary directory for the lifetime of the fixture's commands.
    _workspace: TestDir,
}

impl Fixture {
    /// Share checked-in sources. Only build outputs may be written by the test.
    fn readonly(sub: &str) -> Self {
        let workspace = TestDir::new_empty();
        Self {
            cwd: Path::new(TEST_CASES_ROOT).join(sub),
            target_dir: Some(workspace.join("_build")),
            _workspace: workspace,
        }
    }

    /// Copy the sources into a private workspace and let Moon choose its normal build path.
    fn copied(sub: &str) -> Self {
        let workspace = TestDir::new(sub);
        Self {
            cwd: workspace.as_ref().to_owned(),
            target_dir: None,
            _workspace: workspace,
        }
    }

    fn join(&self, sub: impl AsRef<Path>) -> PathBuf {
        self.cwd.join(sub)
    }

    /// Change the command's working directory while keeping the fixture alive.
    fn in_subdir(mut self, sub: &str) -> Self {
        self.cwd.push(sub);
        self
    }
}

/// Working directory and optional build target for the Moon command helpers.
pub trait TestCommandDir {
    fn cwd(&self) -> &Path;
    fn target_dir(&self) -> Option<&Path> {
        None
    }
}

impl<T: AsRef<Path> + ?Sized> TestCommandDir for T {
    fn cwd(&self) -> &Path {
        self.as_ref()
    }
}

impl TestCommandDir for Fixture {
    fn cwd(&self) -> &Path {
        &self.cwd
    }
    fn target_dir(&self) -> Option<&Path> {
        self.target_dir.as_deref()
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

pub fn moon_cmd(dir: &impl TestCommandDir) -> snapbox::cmd::Command {
    snapbox::cmd::Command::from_std(moon_process_cmd(dir))
}

pub fn moon_process_cmd(dir: &impl TestCommandDir) -> std::process::Command {
    let mut cmd = std::process::Command::new(moon_bin());
    cmd.env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .env("MOONRUN_OVERRIDE", moonrun_bin())
        .env("MOON_DEP_CACHE", "off")
        .current_dir(dir.cwd());
    if let Some(target) = dir.target_dir() {
        cmd.arg("--target-dir").arg(target);
    }
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
    dir: &impl TestCommandDir,
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
    dir: &impl TestCommandDir,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_output_without_replace(
        dir,
        args,
        [] as [(&str, &str); 0],
        ExpectedStatus::Success,
        OutputStream::Stdout,
    );
    replace_dir(&s, dir.cwd())
}

#[track_caller]
pub fn get_stdout_with_envs(
    dir: &impl TestCommandDir,
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
    replace_dir(&s, dir.cwd())
}

#[track_caller]
pub fn get_stderr(
    dir: &impl TestCommandDir,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_output_without_replace(
        dir,
        args,
        [] as [(&str, &str); 0],
        ExpectedStatus::Success,
        OutputStream::Stderr,
    );
    replace_dir(&s, dir.cwd())
}

#[track_caller]
pub fn get_stderr_with_envs(
    dir: &impl TestCommandDir,
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
    replace_dir(&s, dir.cwd())
}

#[track_caller]
pub fn assert_success(
    dir: &impl TestCommandDir,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) {
    moon_cmd(dir).args(args).assert().success();
}

#[track_caller]
pub fn get_err_stdout(
    dir: &impl TestCommandDir,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_output_without_replace(
        dir,
        args,
        [] as [(&str, &str); 0],
        ExpectedStatus::Failure,
        OutputStream::Stdout,
    );
    replace_dir(&s, dir.cwd())
}

#[track_caller]
pub fn get_err_stderr(
    dir: &impl TestCommandDir,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_output_without_replace(
        dir,
        args,
        [] as [(&str, &str); 0],
        ExpectedStatus::Failure,
        OutputStream::Stderr,
    );
    replace_dir(&s, dir.cwd())
}

#[track_caller]
pub fn get_err_stderr_with_envs(
    dir: &impl TestCommandDir,
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
    replace_dir(&s, dir.cwd())
}
