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
