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

use std::path::{Path, PathBuf};

use expect_test::{Expect, expect};

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

    fn join(&self, sub: &str) -> PathBuf {
        self.path.path().join(sub)
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        self.path.path()
    }
}

fn copy(src: &Path, dest: &Path) -> anyhow::Result<()> {
    if src.is_dir() {
        if !dest.exists() {
            std::fs::create_dir_all(dest)?;
        }
        for entry in walkdir::WalkDir::new(src) {
            let entry = entry?;
            let path = entry.path();
            let relative_path = path.strip_prefix(src)?;
            let dest_path = dest.join(relative_path);
            if path.is_dir() {
                if !dest_path.exists() {
                    std::fs::create_dir_all(dest_path)?;
                }
            } else {
                std::fs::copy(path, dest_path)?;
            }
        }
    } else {
        std::fs::copy(src, dest)?;
    }
    Ok(())
}

fn check(actual: &str, expect: Expect) {
    expect.assert_eq(actual)
}

#[test]
fn test_moonrun_version() {
    let out = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moonrun"))
        .arg("--version")
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();
    let s = std::str::from_utf8(&out).unwrap().to_string();
    assert!(s.contains("moonrun"));
}

#[test]
fn test_moonrun_wasm_stack_trace() {
    let dir = TestDir::new("test_stack_trace.in");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .arg("build")
        .assert()
        .success();

    let main_wasm = dir.join("target/wasm-gc/debug/build/main/main.wasm");

    let out = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moonrun"))
        .arg(&main_wasm)
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();
    let s = std::str::from_utf8(&out).unwrap().to_string();
    check(
        &s,
        expect![[r#"
            RuntimeError: unreachable
                at moonbitlang/core/abort.abort|Unit| (wasm://wasm/2cf5837e:wasm-function[1]:0x87)
                at *main*/1 (wasm://wasm/2cf5837e:wasm-function[2]:0x93)
        "#]],
    );

    let out = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moonrun"))
        .arg(&main_wasm)
        .arg("--no-stack-trace")
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();
    let s = std::str::from_utf8(&out).unwrap().to_string();
    check(
        &s,
        expect![[r#"
            RuntimeError: unreachable
        "#]],
    );
}

#[test]
fn test_moon_run_with_cli_args() {
    let dir = TestDir::new("test_cli_args.in");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .arg("build")
        .assert()
        .success();

    let wasm_file = dir.join("target/wasm-gc/debug/build/main/main.wasm");

    // `argv` passed to CLI is:
    // <wasm_file> <...rest argv to moonrun>

    // Assert it has the WASM file as argv[0]
    let out = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moonrun"))
        .arg(&wasm_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();
    let s = std::str::from_utf8(&out).unwrap().to_string();

    assert!(s.contains(".wasm"));
    assert!(!s.contains("moonrun"));

    // Assert it passes the rest verbatim
    let out = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moonrun"))
        .arg(&wasm_file)
        .arg("--")
        .args(["中文", "😄👍", "hello", "1242"])
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();
    let s = std::str::from_utf8(&out).unwrap().to_string();

    assert!(!s.contains("moonrun"));
    assert!(!s.contains("--"));
    assert!(s.contains(r#".wasm", "中文", "😄👍", "hello", "1242""#));

    let out = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moonrun"))
        .arg(&wasm_file)
        .arg("--no-stack-trace") // this ia an arg accepted by moonrun
        .arg("--")
        .args(["--arg1", "--arg2", "arg3"])
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();
    let s = std::str::from_utf8(&out).unwrap().to_string();

    assert!(!s.contains("moonrun"));
    assert!(!s.contains("--no-stack-trace"));
    assert!(s.contains(r#".wasm", "--arg1", "--arg2", "arg3""#))
}

#[test]
fn test_moon_run_with_read_bytes_from_stdin() {
    let dir = TestDir::new("test_read_bytes.in");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .arg("build")
        .assert()
        .success();

    let wasm_file = dir.join("target/wasm-gc/debug/build/main/main.wasm");

    let out = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moonrun"))
        .arg(&wasm_file)
        .stdin("中文😄👍hello1242")
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();
    let s = std::str::from_utf8(&out).unwrap().to_string();
    assert!(s.trim() == "中文😄👍hello1242".len().to_string());

    let out = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moonrun"))
        .arg(&wasm_file)
        .stdin("")
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();
    let s = std::str::from_utf8(&out).unwrap().to_string();
    assert!(s.trim() == "0");
}

#[test]
fn test_moon_run_with_is_windows() {
    let dir = TestDir::new("test_os_platform_detection");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .arg("build")
        .assert()
        .success();

    let wasm_file = dir.join("target/wasm-gc/release/build/main/main.wasm");

    let out = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moonrun"))
        .arg(&wasm_file)
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();

    let s = std::str::from_utf8(&out).unwrap().to_string();
    let actual_result = s.trim();
    let expected_result = if std::env::consts::OS == "windows" {
        "1"
    } else {
        "0"
    };
    assert!(actual_result == expected_result);
}
