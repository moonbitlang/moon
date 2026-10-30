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

use std::path::{Path, PathBuf};

fn compile_runner(dir: &Path) -> PathBuf {
    let runner = dir.join(format!("testcli-runner{}", std::env::consts::EXE_SUFFIX));
    snapbox::cmd::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/testcli_runner.rs"))
        .arg("-o")
        .arg(&runner)
        .assert()
        .success();
    runner
}

fn testcli_command(dir: &Path, runner: &Path) -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .current_dir(dir)
        .env("MOON_TOOLCHAIN_ROOT", dir)
        .env("MOONRUN_OVERRIDE", runner)
        .env("TESTCLI_EXPECTED_CWD", dir)
        .env("TESTCLI_EXIT_CODE", "0")
}

#[test]
fn testcli_preserves_arguments_streams_and_exit_code_without_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let runner = compile_runner(dir.path());
    // The fixture checks the selected wasm path and runner separator. There is
    // deliberately no project or wasm file: loading belongs to the runner.
    testcli_command(dir.path(), &runner)
        .env("TESTCLI_EXIT_CODE", "37")
        .args([
            "testcli",
            "--help",
            "--version",
            "-C",
            "other directory",
            "--trace",
            "--quiet",
            "--verbose",
            "--dry-run",
            "--target-dir",
            "output",
            "--unknown-option=value",
            "--",
            "",
            "two words",
            "--",
        ])
        .stdin("runner stdin\n")
        .assert()
        .code(37)
        .stdout_eq(snapbox::str![[r#"
arg="--help"
arg="--version"
arg="-C"
arg="other directory"
arg="--trace"
arg="--quiet"
arg="--verbose"
arg="--dry-run"
arg="--target-dir"
arg="output"
arg="--unknown-option=value"
arg="--"
arg=""
arg="two words"
arg="--"
runner stdin

"#]])
        .stderr_eq("runner stderr\n");
}

#[test]
fn testcli_resolves_relative_runner_overrides_from_the_effective_directory() {
    let dir = tempfile::tempdir().unwrap();
    let subdir = dir.path().join("subdir");
    std::fs::create_dir(&subdir).unwrap();
    let runner = compile_runner(&subdir);
    // Exercise both an explicit relative path and a bare name found through a
    // relative PATH entry. Neither runner is available from the initial cwd.
    for runner_override in [
        Path::new(".").join(runner.file_name().unwrap()),
        PathBuf::from(runner.file_name().unwrap()),
    ] {
        testcli_command(dir.path(), &runner_override)
            .env("PATH", ".")
            .env("TESTCLI_EXPECTED_CWD", &subdir)
            .args(["-qCsubdir", "testcli"])
            .assert()
            .success()
            .stdout_eq("")
            .stderr_eq("runner stderr\n");
    }
}

#[cfg(unix)]
#[test]
fn testcli_preserves_non_utf8_arguments() {
    use std::os::unix::ffi::OsStringExt;

    let dir = tempfile::tempdir().unwrap();
    let runner = compile_runner(dir.path());
    testcli_command(dir.path(), &runner)
        .arg("testcli")
        .arg(std::ffi::OsString::from_vec(b"non-utf8-\xff".to_vec()))
        .assert()
        .success()
        .stdout_eq(snapbox::Data::text("arg=\"non-utf8-\\xFF\"\n").raw())
        .stderr_eq("runner stderr\n");
}

#[test]
fn testcli_reports_runner_launch_errors() {
    let dir = tempfile::tempdir().unwrap();
    testcli_command(dir.path(), &dir.path().join("missing-runner"))
        .arg("testcli")
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Error: failed to delegate to external command

Caused by:
...
"#]]);
}
