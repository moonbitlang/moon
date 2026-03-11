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

pub(crate) use support::{build_graph, dry_run_utils, util};

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

struct MoonTestOutput {
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[track_caller]
fn run_moon_command(
    dir: &Path,
    args: &[std::ffi::OsString],
    envs: &[(std::ffi::OsString, std::ffi::OsString)],
) -> MoonTestOutput {
    if let Some(output) = support::persistent::maybe_run_moon(dir, args, envs) {
        return MoonTestOutput {
            exit_code: output.exit_code,
            stdout: output.stdout,
            stderr: output.stderr,
        };
    }

    let output = std::process::Command::new(moon_bin())
        .envs(envs.iter().map(|(name, value)| (name, value)))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap_or_else(|err| {
            panic!(
                "failed to run `moon {}` in {}: {err}",
                args.iter()
                    .map(|arg| arg.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
                dir.display()
            )
        });

    MoonTestOutput {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

#[track_caller]
fn assert_exit_code(
    output: &MoonTestOutput,
    expected: i32,
    dir: &Path,
    args: &[std::ffi::OsString],
) {
    assert_eq!(
        output.exit_code,
        expected,
        "unexpected exit code for `moon {}` in {}\nstdout:\n{}\nstderr:\n{}",
        args.iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" "),
        dir.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[track_caller]
fn get_stdout_without_replace(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<std::ffi::OsStr>, impl AsRef<std::ffi::OsStr>)>,
) -> String {
    let dir = dir.as_ref();
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<_>>();
    let envs = envs
        .into_iter()
        .map(|(name, value)| (name.as_ref().to_os_string(), value.as_ref().to_os_string()))
        .collect::<Vec<_>>();
    let out = run_moon_command(dir, &args, &envs);
    assert_exit_code(&out, 0, dir, &args);

    std::str::from_utf8(&out.stdout).unwrap().to_string()
}

#[track_caller]
fn get_stderr_without_replace(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<std::ffi::OsStr>, impl AsRef<std::ffi::OsStr>)>,
) -> String {
    let dir = dir.as_ref();
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<_>>();
    let envs = envs
        .into_iter()
        .map(|(name, value)| (name.as_ref().to_os_string(), value.as_ref().to_os_string()))
        .collect::<Vec<_>>();
    let out = run_moon_command(dir, &args, &envs);
    assert_exit_code(&out, 0, dir, &args);

    std::str::from_utf8(&out.stderr).unwrap().to_string()
}

#[track_caller]
fn get_err_stdout_without_replace(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<std::ffi::OsStr>, impl AsRef<std::ffi::OsStr>)>,
) -> String {
    let dir = dir.as_ref();
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<_>>();
    let envs = envs
        .into_iter()
        .map(|(name, value)| (name.as_ref().to_os_string(), value.as_ref().to_os_string()))
        .collect::<Vec<_>>();
    let out = run_moon_command(dir, &args, &envs);
    assert_ne!(
        out.exit_code,
        0,
        "expected failure for `moon {}` in {}\nstdout:\n{}\nstderr:\n{}",
        args.iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" "),
        dir.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    std::str::from_utf8(&out.stdout).unwrap().to_string()
}

#[track_caller]
fn get_err_stderr_without_replace(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<std::ffi::OsStr>, impl AsRef<std::ffi::OsStr>)>,
) -> String {
    let dir = dir.as_ref();
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<_>>();
    let envs = envs
        .into_iter()
        .map(|(name, value)| (name.as_ref().to_os_string(), value.as_ref().to_os_string()))
        .collect::<Vec<_>>();
    let out = run_moon_command(dir, &args, &envs);
    assert_ne!(
        out.exit_code,
        0,
        "expected failure for `moon {}` in {}\nstdout:\n{}\nstderr:\n{}",
        args.iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" "),
        dir.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    std::str::from_utf8(&out.stderr).unwrap().to_string()
}

#[track_caller]
pub fn get_stdout(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_stdout_without_replace(dir, args, [] as [(&str, &str); 0]);
    replace_dir(&s, dir)
}

#[track_caller]
pub fn get_stdout_with_envs(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
    envs: impl IntoIterator<Item = (impl AsRef<std::ffi::OsStr>, impl AsRef<std::ffi::OsStr>)>,
) -> String {
    let s = get_stdout_without_replace(dir, args, envs);
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
pub fn get_stderr(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_stderr_without_replace(dir, args, [] as [(&str, &str); 0]);
    replace_dir(&s, dir)
}

#[track_caller]
pub fn get_err_stdout(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_err_stdout_without_replace(dir, args, [] as [(&str, &str); 0]);
    replace_dir(&s, dir)
}

#[track_caller]
pub fn get_err_stderr(
    dir: &impl AsRef<std::path::Path>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> String {
    let s = get_err_stderr_without_replace(dir, args, [] as [(&str, &str); 0]);
    replace_dir(&s, dir)
}
