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
fn test_moonrun_stack_trace_preserves_wasm_location() {
    let dir = tempfile::tempdir().unwrap();
    let wasm_path = dir.path().join("trap.wasm");
    let wasm = wat::parse_str(r#"(module (func $crash (export "_start") unreachable))"#).unwrap();
    std::fs::write(&wasm_path, wasm).unwrap();

    for source_map in [None, Some("invalid source map")] {
        if let Some(source_map) = source_map {
            std::fs::write(dir.path().join("trap.wasm.map"), source_map).unwrap();
        }
        snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
            .arg(&wasm_path)
            .assert()
            .failure()
            .stdout_eq("")
            .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at crash ([..]0x[..])

"#]]);
    }

    std::fs::write(
        dir.path().join("trap.wasm.map"),
        r#"{"sources":["main.mbt"],"mappings":"AAAA"}"#,
    )
    .unwrap();
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_path)
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at crash main.mbt:1 ([..]0x[..])

"#]]);

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_path)
        .arg("--no-stack-trace")
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq("RuntimeError: unreachable\n");
}

#[test]
fn test_moonrun_wasm_stack_trace() {
    let dir = TestDir::new("test_stack_trace.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();

    let main_wasm = dir.join("_build/wasm-gc/debug/build/main/main.wasm");
    let redactions = moon_test_util::stack_trace::stack_trace_redactions(dir.as_ref());
    let assert = snapbox::Assert::new().redact_with(redactions);

    fn moonrun_stack_trace_case(
        main_wasm: &std::path::Path,
        mode: Option<&str>,
        assert: snapbox::Assert,
    ) -> snapbox::cmd::Command {
        let cmd = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
            .with_assert(assert)
            .arg(main_wasm);
        if let Some(mode) = mode {
            cmd.arg("--").arg(mode)
        } else {
            cmd
        }
    }

    moonrun_stack_trace_case(&main_wasm, None, assert.clone())
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @username/hello/main.abort_with_tuple [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])
    at @username/hello/main.default_abort_chain [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])
    at __moonbit_main [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])

"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("abort-generic-int"), assert.clone())
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @username/hello/main.abort_generic[Int] [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])
    at __moonbit_main [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])

"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("abort-generic-tuple"), assert.clone())
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @username/hello/main.abort_generic[(Int, String)] [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])
    at __moonbit_main [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])

"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("abort-method"), assert.clone())
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @username/hello/main.CrashBox::abort_method [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])
    at __moonbit_main [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])

"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("abort-closure"), assert.clone())
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @username/hello/main.abort_via_closure.inner[stamp=[..]] [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])
    at @username/hello/main.abort_via_closure [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])
    at __moonbit_main [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])

"#]]);

    moonrun_stack_trace_case(&main_wasm, Some("panic-result"), assert.clone())
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
RuntimeError: unreachable
    at @username/hello/main.panic_with_result [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])
    at __moonbit_main [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])

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
    let redactions = moon_test_util::stack_trace::stack_trace_redactions(dir.as_ref());
    let assert = snapbox::Assert::new().redact_with(redactions);

    // The native exception callback adds no JavaScript frame, so both engines
    // now retain the outer Wasm test-driver frame.
    moon_test_case(&dir, &["--filter", "stacktrace test abort closure"])
        .with_assert(assert.clone())
        .assert()
        .failure()
        .stdout_eq(snapbox::str![[r#"
[username/hello] test main/main.mbt:[..] ("stacktrace test abort closure") failed: Error
    at @username/hello/main.abort_via_closure.inner[stamp=[..]] [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])
    at @username/hello/main.abort_via_closure [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])
    at @username/hello/main.__test_6d61696e2e6d6274_2 [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])
    at @username/hello/main.__test_6d61696e2e6d6274_2.dyncall ([..]0x[..])
    at @username/hello/main.moonbit_test_driver_internal_catch_error [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER] ([..]0x[..])
    at impl @username/hello/main.MoonBit_Test_Driver for @username/hello/main.MoonBit_Test_Driver_Internal_No_Args with run_test [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER] ([..]0x[..])
    at @username/hello/main.moonbit_test_driver_internal_do_execute [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER] ([..]0x[..])
    at @username/hello/main.moonbit_test_driver_internal_execute [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER] ([..]0x[..])
    at @username/hello/main.moonbit_test_driver_internal_execute.wrapper [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER] ([..]0x[..])
Total tests: 1, passed: 0, failed: 1.

"#]]);

    moon_test_case(&dir, &["main/main.mbt", "--index", "1"])
        .with_assert(assert.clone())
        .assert()
        .failure()
        .stdout_eq(snapbox::str![[r#"
[username/hello] test main/main.mbt:[..] ("stacktrace test abort method") failed: Error
    at @username/hello/main.CrashBox::abort_method [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])
    at @username/hello/main.__test_6d61696e2e6d6274_1 [..]/main/main.mbt[LINE_NUMBER] ([..]0x[..])
    at @username/hello/main.__test_6d61696e2e6d6274_1.dyncall ([..]0x[..])
    at @username/hello/main.moonbit_test_driver_internal_catch_error [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER] ([..]0x[..])
    at impl @username/hello/main.MoonBit_Test_Driver for @username/hello/main.MoonBit_Test_Driver_Internal_No_Args with run_test [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER] ([..]0x[..])
    at @username/hello/main.moonbit_test_driver_internal_do_execute [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER] ([..]0x[..])
    at @username/hello/main.moonbit_test_driver_internal_execute [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER] ([..]0x[..])
    at @username/hello/main.moonbit_test_driver_internal_execute.wrapper [..]/main/__generated_driver_for_internal_test.mbt[LINE_NUMBER] ([..]0x[..])
Total tests: 1, passed: 0, failed: 1.

"#]]);
}
