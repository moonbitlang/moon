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

fn moon_cmd() -> snapbox::cmd::Command {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../moon/Cargo.toml");
    snapbox::cmd::Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(manifest_path)
        .arg("--bin")
        .arg("moon")
        .arg("--")
}

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

#[test]
fn test_moonrun_version() {
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg("--version")
        .assert()
        .success()
        .stdout_eq("moonrun [..]\n");
}

#[test]
fn test_moonrun_wasm_stack_trace() {
    let dir = TestDir::new("test_stack_trace.in");

    moon_cmd().current_dir(&dir).arg("build").assert().success();

    let main_wasm = dir.join("_build/wasm-gc/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&main_wasm)
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @moonbitlang/core/abort.abort[Unit] [..]/abort/abort.mbt:29
    at @moonbitlang/core/builtin.abort[Unit] [..]/builtin/intrinsics.mbt:70
    at @__moonbit_main [..]/main/main.mbt:20
...
"#]]);

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&main_wasm)
        .arg("--no-stack-trace")
        .assert()
        .failure()
        .stderr_eq("RuntimeError: unreachable\n");
}

#[test]
fn test_moon_run_with_cli_args() {
    let dir = TestDir::new("test_cli_args.in");

    moon_cmd().current_dir(&dir).arg("build").assert().success();

    let wasm_file = dir.join("_build/wasm-gc/debug/build/main/main.wasm");

    // `argv` passed to CLI is:
    // <wasm_file> <...rest argv to moonrun>

    // Assert it has the WASM file as argv[0]
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .success()
        .stdout_eq("[\"[..]/_build/wasm-gc/debug/build/main/main.wasm\"]\n");

    // Assert it passes the rest verbatim
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .arg("--")
        .args(["中文", "😄👍", "hello", "1242"])
        .assert()
        .success()
        .stdout_eq("[\"[..]/_build/wasm-gc/debug/build/main/main.wasm\", \"中文\", \"😄👍\", \"hello\", \"1242\"]\n");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .arg("--no-stack-trace") // this ia an arg accepted by moonrun
        .arg("--")
        .args(["--arg1", "--arg2", "arg3"])
        .assert()
        .success()
        .stdout_eq("[\"[..]/_build/wasm-gc/debug/build/main/main.wasm\", \"--arg1\", \"--arg2\", \"arg3\"]\n");
}

#[test]
fn test_moon_run_with_read_bytes_from_stdin() {
    let dir = TestDir::new("test_read_bytes.in");

    moon_cmd().current_dir(&dir).arg("build").assert().success();

    let wasm_file = dir.join("_build/wasm-gc/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .stdin("中文😄👍hello1242")
        .assert()
        .success()
        .stdout_eq(format!("{}\n", "中文😄👍hello1242".len()));

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .stdin("")
        .assert()
        .success()
        .stdout_eq("0\n");
}

#[test]
fn test_moon_run_with_is_windows() {
    let dir = TestDir::new("test_os_platform_detection");

    moon_cmd().current_dir(&dir).arg("build").assert().success();

    let wasm_file = dir.join("_build/wasm-gc/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .success()
        .stdout_eq(if std::env::consts::OS == "windows" {
            "1\n"
        } else {
            "0\n"
        });
}

#[test]
fn test_moon_fmt_skips_prebuild_output() {
    // Prepare a temp copy of the test case
    let dir = TestDir::new("test_fmt_skip_prebuild_output");

    // The prebuild command is a NOOP; we intentionally wrote a sloppy file as the "generated" output.
    // Ensure the source remains sloppy after fmt (formatter must skip prebuild outputs).
    let generated_src = dir.join("main/generated.mbt");
    let original = std::fs::read_to_string(&generated_src).expect("read generated.mbt");

    // Run: moon fmt
    moon_cmd()
        .current_dir(&dir)
        .args(["fmt"])
        .assert()
        .success();

    let after = std::fs::read_to_string(&generated_src).expect("read generated.mbt");
    assert_eq!(original, after, "Formatter should skip prebuild outputs");
}
