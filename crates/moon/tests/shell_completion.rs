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

use std::path::PathBuf;
use std::process::Command;

fn completion_test(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("shell_completion")
        .join(name)
}

fn assert_command_succeeds(command: &mut Command) {
    let invocation = format!("{command:?}");
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("failed to run {invocation}: {error}"));

    assert!(
        output.status.success(),
        "{invocation} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[cfg(target_os = "linux")]
#[test]
fn bash_completion_behaves_as_expected() {
    assert_command_succeeds(
        Command::new("bash")
            .arg(completion_test("bash.sh"))
            .arg(env!("CARGO_BIN_EXE_moon")),
    );
}

#[cfg(target_os = "macos")]
#[test]
fn zsh_completion_behaves_as_expected() {
    assert_command_succeeds(
        Command::new("zsh")
            .arg(completion_test("zsh.zsh"))
            .arg(env!("CARGO_BIN_EXE_moon")),
    );
}

#[cfg(windows)]
#[test]
fn powershell_completion_behaves_as_expected() {
    assert_command_succeeds(
        Command::new("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(completion_test("powershell.ps1"))
            .arg(env!("CARGO_BIN_EXE_moon")),
    );
}
