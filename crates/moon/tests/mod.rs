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

mod support;
mod test_cases;

use moonbuild_debug::graph::ENV_VAR;
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

pub fn moon_cmd(dir: &impl AsRef<Path>) -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(moon_bin())
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .current_dir(dir)
}

pub fn moon_process_cmd(dir: &impl AsRef<Path>) -> std::process::Command {
    let mut cmd = std::process::Command::new(moon_bin());
    cmd.env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
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

/// Snapshot the dry run graph output to a file, returning the regular stdout
/// and outputting the graph to the specified file via an environment variable.
///
/// Note: You must pass a dry-run related command in `args`.
#[track_caller]
pub fn snap_dry_run_graph(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    to_file: &impl AsRef<std::path::Path>,
) -> String {
    get_stdout_with_envs(
        dir,
        args,
        [(ENV_VAR, to_file.as_ref().to_string_lossy().into_owned())],
    )
}

#[track_caller]
pub(crate) fn assert_dry_run_graph(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    expected: impl build_graph::IExpect,
) {
    let graph = tempfile::NamedTempFile::new().expect("dry-run graph temp file should create");
    snap_dry_run_graph(dir, args, &graph.path());
    build_graph::compare_graphs(graph.path(), expected);
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
