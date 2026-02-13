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

struct TestDir(moon_test_util::test_dir::TestDir);

impl TestDir {
    // create a new TestDir with the test directory in tests/test_cases/<sub>
    fn new(sub: &str) -> Self {
        let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
        Self(moon_test_util::test_dir::TestDir::from_case_root(
            case_root, sub, false,
        ))
    }

    fn join(&self, sub: &str) -> PathBuf {
        self.0.join(sub)
    }
}

impl AsRef<std::path::Path> for TestDir {
    fn as_ref(&self) -> &std::path::Path {
        self.0.as_ref()
    }
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

    fn moonrun_stack_trace_case(
        main_wasm: &std::path::Path,
        mode: Option<&str>,
    ) -> snapbox::cmd::Command {
        let cmd = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun")).arg(main_wasm);
        if let Some(mode) = mode {
            cmd.arg("--").arg(mode)
        } else {
            cmd
        }
    }

    moonrun_stack_trace_case(&main_wasm, None)
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @moonbitlang/core/abort.abort[Unit] [..]/abort/abort.mbt:[..]
    at @moonbitlang/core/builtin.abort[Unit] [..]/builtin/intrinsics.mbt:[..]
    at @username/hello/main.abort_with_tuple [..]/main/main.mbt:[..]
    at @username/hello/main.default_abort_chain [..]/main/main.mbt:[..]
    at @__moonbit_main [..]/main/main.mbt:[..]
"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("abort-generic-int"))
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @moonbitlang/core/abort.abort[Int] [..]/abort/abort.mbt:[..]
    at @moonbitlang/core/builtin.abort[Int] [..]/builtin/intrinsics.mbt:[..]
    at @username/hello/main.abort_generic[Int] [..]/main/main.mbt:[..]
    at @__moonbit_main [..]/main/main.mbt:[..]
"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("abort-generic-tuple"))
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @moonbitlang/core/abort.abort[(Int, String)] [..]/abort/abort.mbt:[..]
    at @moonbitlang/core/builtin.abort[(Int, String)] [..]/builtin/intrinsics.mbt:[..]
    at @username/hello/main.abort_generic[(Int, String)] [..]/main/main.mbt:[..]
    at @__moonbit_main [..]/main/main.mbt:[..]
"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("abort-method"))
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @moonbitlang/core/abort.abort[UInt] [..]/abort/abort.mbt:[..]
    at @moonbitlang/core/builtin.abort[UInt] [..]/builtin/intrinsics.mbt:[..]
    at @username/hello/main.CrashBox::abort_method [..]/main/main.mbt:[..]
    at @__moonbit_main [..]/main/main.mbt:[..]
"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("abort-closure"))
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @moonbitlang/core/abort.abort[Int] [..]/abort/abort.mbt:[..]
    at @moonbitlang/core/builtin.abort[Int] [..]/builtin/intrinsics.mbt:[..]
    at @username/hello/main.abort_via_closure.inner/[..] [..]/main/main.mbt:[..]
    at @username/hello/main.abort_via_closure [..]/main/main.mbt:[..]
    at @__moonbit_main [..]/main/main.mbt:[..]
"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("panic-result"))
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @username/hello/main.panic_with_result [..]/main/main.mbt:[..]
    at @__moonbit_main [..]/main/main.mbt:[..]
"#]]);

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&main_wasm)
        .arg("--no-stack-trace")
        .assert()
        .failure()
        .stderr_eq("RuntimeError: unreachable\n");
}

#[test]
fn test_moonrun_wasm_stack_trace_in_test_blocks() {
    let dir = TestDir::new("test_stack_trace.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["test", "--target", "wasm-gc", "--build-only"])
        .assert()
        .success();

    fn moon_test_case(dir: &TestDir, args: &[&str]) -> snapbox::cmd::Command {
        moon_cmd()
            .current_dir(dir)
            .arg("test")
            .arg("--target")
            .arg("wasm-gc")
            .args(args)
    }

    moon_test_case(&dir, &["--filter", "stacktrace test abort closure"])
        .assert()
        .failure()
        .stdout_eq(snapbox::str![[r#"
[username/hello] test main/main.mbt:[..] ("stacktrace test abort closure") failed: Error
    at throw
    at @moonbitlang/core/abort.abort[Int] [..]/abort/abort.mbt:[..]
    at @moonbitlang/core/builtin.abort[Int] [..]/builtin/intrinsics.mbt:[..]
    at @username/hello/main.abort_via_closure.inner/[..] [..]/main/main.mbt:[..]
    at @username/hello/main.abort_via_closure [..]/main/main.mbt:[..]
    at @username/hello/main.__test_6d61696e2e6d6274_2 [..]/main/main.mbt:[..]
    at @username/hello/main.__test_6d61696e2e6d6274_2.dyncall
    at @username/hello/main.moonbit_test_driver_internal_catch_error [..]/main/__generated_driver_for_internal_test.mbt:[..]
    at impl @username/hello/main.MoonBit_Test_Driver for @username/hello/main.MoonBit_Test_Driver_Internal_No_Args with run_test [..]/main/__generated_driver_for_internal_test.mbt:[..]
    at @username/hello/main.moonbit_test_driver_internal_do_execute [..]/main/__generated_driver_for_internal_test.mbt:[..]
Total tests: 1, passed: 0, failed: 1.

"#]]);

    moon_test_case(&dir, &["main/main.mbt", "--index", "1"])
        .assert()
        .failure()
        .stdout_eq(snapbox::str![[r#"
[username/hello] test main/main.mbt:[..] ("stacktrace test abort method") failed: Error
    at throw
    at @moonbitlang/core/abort.abort[UInt] [..]/abort/abort.mbt:[..]
    at @moonbitlang/core/builtin.abort[UInt] [..]/builtin/intrinsics.mbt:[..]
    at @username/hello/main.CrashBox::abort_method [..]/main/main.mbt:[..]
    at @username/hello/main.__test_6d61696e2e6d6274_1 [..]/main/main.mbt:[..]
    at @username/hello/main.__test_6d61696e2e6d6274_1.dyncall
    at @username/hello/main.moonbit_test_driver_internal_catch_error [..]/main/__generated_driver_for_internal_test.mbt:[..]
    at impl @username/hello/main.MoonBit_Test_Driver for @username/hello/main.MoonBit_Test_Driver_Internal_No_Args with run_test [..]/main/__generated_driver_for_internal_test.mbt:[..]
    at @username/hello/main.moonbit_test_driver_internal_do_execute [..]/main/__generated_driver_for_internal_test.mbt:[..]
    at @username/hello/main.moonbit_test_driver_internal_execute [..]/main/__generated_driver_for_internal_test.mbt:[..]
Total tests: 1, passed: 0, failed: 1.

"#]]);
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
