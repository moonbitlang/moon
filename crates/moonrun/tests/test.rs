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

fn normalize_source_token(token: &str) -> String {
    let Some(line_sep) = token.rfind(':') else {
        return token.to_string();
    };
    if !token[line_sep + 1..].chars().all(|c| c.is_ascii_digit()) {
        return token.to_string();
    }

    let path = &token[..line_sep];
    if !(path.contains('/') || path.contains('\\')) {
        return token.to_string();
    }

    let parts: Vec<_> = path
        .split(['/', '\\'])
        .filter(|seg| !seg.is_empty())
        .collect();
    let short_path = if parts.len() >= 2 {
        format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
    } else {
        parts
            .last()
            .map(|s| (*s).to_string())
            .unwrap_or_else(|| path.to_string())
    };
    format!("{}:{}", short_path, &token[line_sep + 1..])
}

fn normalize_source_paths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if let Some(space) = line.rfind(' ') {
            out.push_str(&line[..space + 1]);
            out.push_str(&normalize_source_token(&line[space + 1..]));
        } else {
            out.push_str(line);
        }
    }
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
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

pub fn normalize_wasm_trace(text: &str) -> String {
    const PREFIX: &str = "wasm://wasm/";

    let mut result = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Look for the prefix
        if bytes[i..].starts_with(PREFIX.as_bytes()) {
            // Write "wasm://wasm:"
            result.push_str("wasm://wasm:");
            i += PREFIX.len();

            // Skip hex characters (the hash)
            while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                i += 1;
            }

            // Skip the ':' after hash
            if i < bytes.len() && bytes[i] == b':' {
                i += 1;
            }

            // Copy until we hit another ':' (the offset separator)
            while i < bytes.len() && bytes[i] != b':' {
                result.push(bytes[i] as char);
                i += 1;
            }

            // Skip the ':0x...' part (offset)
            if i < bytes.len() && bytes[i] == b':' {
                i += 1; // skip ':'
                // Skip '0x' if present
                if i + 1 < bytes.len() && bytes[i] == b'0' && bytes[i + 1] == b'x' {
                    i += 2;
                }
                // Skip hex digits
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                    i += 1;
                }
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    normalize_source_paths(&result)
}

#[test]
fn test_moonrun_wasm_stack_trace() {
    let dir = TestDir::new("test_stack_trace.in");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .arg("build")
        .assert()
        .success();

    let main_wasm = dir.join("_build/wasm-gc/debug/build/main/main.wasm");

    let out = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moonrun"))
        .arg(&main_wasm)
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();
    // original output:
    //             RuntimeError: unreachable
    //          at wasm://wasm/d858b7fa:wasm-function[19]:0x734
    //          at wasm://wasm/d858b7fa:wasm-function[17]:0x72b
    //          at wasm://wasm/d858b7fa:wasm-function[24]:0x7a3
    let s = std::str::from_utf8(&out).unwrap().to_string();
    // need normalization because the source loc (absolute path now) string in
    // encoded in data section and makes the hash of the .wasm file flaky
    // because the absolute path contains temp dir path
    let normalized_s = normalize_wasm_trace(&s);
    check(
        &normalized_s,
        expect![[r#"
            RuntimeError: unreachable
                at @moonbitlang/core/abort.abort[Unit] abort/abort.mbt:29
                at @moonbitlang/core/builtin.abort[Unit] builtin/intrinsics.mbt:70
                at @__moonbit_main main/main.mbt:20
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

    let wasm_file = dir.join("_build/wasm-gc/debug/build/main/main.wasm");

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

    let wasm_file = dir.join("_build/wasm-gc/debug/build/main/main.wasm");

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

    let wasm_file = dir.join("_build/wasm-gc/debug/build/main/main.wasm");

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

#[test]
fn test_moon_fmt_skips_prebuild_output() {
    // Prepare a temp copy of the test case
    let dir = TestDir::new("test_fmt_skip_prebuild_output");

    // The prebuild command is a NOOP; we intentionally wrote a sloppy file as the "generated" output.
    // Ensure the source remains sloppy after fmt (formatter must skip prebuild outputs).
    let generated_src = dir.join("main/generated.mbt");
    let original = std::fs::read_to_string(&generated_src).expect("read generated.mbt");

    // Run: moon fmt
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin("moon"))
        .current_dir(&dir)
        .args(["fmt"])
        .assert()
        .success();

    let after = std::fs::read_to_string(&generated_src).expect("read generated.mbt");
    assert_eq!(original, after, "Formatter should skip prebuild outputs");
}
