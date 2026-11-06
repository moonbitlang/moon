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

use super::{TestDir, moon_cmd};

#[test]
fn test_moonrun_version() {
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg("--version")
        .assert()
        .success()
        .stdout_eq(format!(
            "moonrun {} ({} {})\n",
            env!("CARGO_PKG_VERSION"),
            env!("VERGEN_GIT_SHA"),
            env!("VERGEN_BUILD_DATE")
        ));
}

#[test]
fn test_moonrun_help_describes_policy_shape() {
    let assert = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg("--help")
        .assert()
        .success();
    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();

    assert!(stdout.contains("Experimental: Sandbox wasm runtime host access"));
    assert!(stdout.contains("JSON, TOML, or .mbtx policy file"));
    assert!(stdout.contains("process.virtualize defaults to false"));
    assert!(stdout.contains("--daemon"));
    assert!(stdout.contains("--virtual-process"));
    assert!(!stdout.contains("--ipc-session"));
    assert!(stdout.contains("deny-by-default mode"));
    assert!(stdout.contains(r#""from_host": ["*"]"#));
    assert!(stdout.contains(r#""read": ["*"]"#));
    assert!(stdout.contains(r#""spawn": true"#));
    assert!(stdout.contains(r#"net.connect containing "api.deepseek.com:443""#));
    assert!(stdout.contains("Hostname connect rules also permit DNS lookup"));
    assert!(stdout.contains("process.allow entries match the exact requested program"));
    assert!(stdout.contains("Omitting args_prefix allows any arguments"));
    assert!(stdout.contains("logical request, not the executable eventually selected"));
    assert!(stdout.contains("ambient filesystem, network, and process access"));
}

#[test]
fn test_moon_run_with_cli_args() {
    let dir = TestDir::new("test_cli_args.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();

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
        .stdout_eq(
            "[\n  \"[..]/_build/wasm-gc/debug/build/main/main.wasm\",\n  \"中文\",\n  \"😄👍\",\n  \"hello\",\n  \"1242\",\n]\n",
        );

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .arg("--no-stack-trace") // this ia an arg accepted by moonrun
        .arg("--")
        .args(["--arg1", "--arg2", "arg3"])
        .assert()
        .success()
        .stdout_eq(
            "[\n  \"[..]/_build/wasm-gc/debug/build/main/main.wasm\",\n  \"--arg1\",\n  \"--arg2\",\n  \"arg3\",\n]\n",
        );
}

#[test]
fn test_moonrun_exits_with_guest_exit_code() {
    let dir = TestDir::new("test_cli_args.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(dir.join("_build/wasm-gc/debug/build/main/main.wasm"))
        .arg("--")
        .arg("exit-7")
        .assert()
        .code(7)
        .stdout_eq("")
        .stderr_eq("");
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
