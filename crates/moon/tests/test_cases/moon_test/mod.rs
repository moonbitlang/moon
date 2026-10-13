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

mod patch;
#[cfg(unix)]
mod use_cc_for_native_release;
mod with_cfg;

use expect_test::expect_file;

use crate::dry_run_utils::assert_lines_in_order;

use super::*;

#[test]
fn test_moon_test_succ() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("NO_COLOR", "1") };
    let dir = TestDir::new("moon_test/succ");
    check(
        get_stdout(&dir, ["test", "-v", "--sort-input", "--no-parallelize"]),
        expect![[r#"
            [moontest] test lib/hello_wbtest.mbt:1 (#0) ok
            [moontest] test lib2/hello_wbtest.mbt:1 (#0) ok
            [moontest] test lib2/nested/lib_wbtest.mbt:1 (#0) ok
            [moontest] test lib2/nested/lib_wbtest.mbt:7 (#1) ok
            [moontest] test lib3/hello_wbtest.mbt:1 (#0) ok
            [moontest] test lib4/hello_wbtest.mbt:1 (#0) ok
            Total tests: 6, passed: 6, failed: 0.
        "#]],
    );
}

#[test]
#[cfg(not(windows))]
fn test_moon_test_succ_llvm() {
    let dir = TestDir::new("moon_test/succ");
    let output = moon_cmd(&dir)
        .env("MOON_OVERRIDE", moon_bin())
        .args([
            "test",
            "--target",
            "llvm",
            "--sort-input",
            "--no-parallelize",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    check(
        std::str::from_utf8(&output).unwrap(),
        expect![[r#"
            Total tests: 6, passed: 6, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_hello_exec() {
    let dir = TestDir::new("moon_test/hello_exec");
    check(
        get_stdout(&dir, ["run", "--target", "wasm-gc", "main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--target", "wasm-gc", "-v"]),
        expect![[r#"
            this is lib test
            [moonbitlang/hello] test lib/hello_wbtest.mbt:1 (#0) ok
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    build_graph::assert(
        moon_cmd(&dir).args([
            "test",
            "--target",
            "wasm-gc",
            "--dry-run",
            "--debug",
            "--sort-input",
        ]),
        expect_file!["moon_test_hello_exec_graph.jsonl.snap"],
    );
}

#[test]
fn test_moon_test_hello_exec_fntest() {
    let dir = TestDir::new("moon_test/hello_exec_fntest");
    check(
        get_stdout(&dir, ["run", "--target", "wasm-gc", "main"]),
        expect![[r#"
            init in main/main.mbt
        "#]],
    );

    build_graph::assert(
        moon_cmd(&dir).args([
            "test",
            "--target",
            "wasm-gc",
            "-v",
            "--dry-run",
            "--sort-input",
        ]),
        expect_file!["moon_test_hello_exec_fntest_graph.jsonl.snap"],
    );

    let test_out = get_stdout(
        &dir,
        [
            "test",
            "--target",
            "wasm-gc",
            "-v",
            "--sort-input",
            "--no-parallelize",
        ],
    );
    assert_lines_in_order(
        &test_out,
        r"
test in lib/hello.mbt
test in lib/hello_test.mbt
Total tests: 2, passed: 2, failed: 0.
    ",
    );
    assert_lines_in_order(
        &test_out,
        r"
[moonbitlang/hello] test lib/hello.mbt:5 (#0) ok
[moonbitlang/hello] test lib/hello_wbtest.mbt:1 (#0) ok
    ",
    );
}

#[test]
fn test_moon_test_hello_lib() {
    let dir = TestDir::new("moon_test/hello_lib");
    check(
        get_stdout(&dir, ["test", "-v"]),
        expect![[r#"
            [moonbitlang/hello] test lib/hello_wbtest.mbt:1 (#0) ok
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    )
}

#[test]
fn test_moon_test_runs_from_module_root() {
    let dir = TestDir::new("moon_test/test_cwd");
    let lib_dir = dir.join("lib");

    check(
        get_stdout(
            &lib_dir,
            [
                "-C",
                "..",
                "test",
                "--target",
                "js",
                "--no-parallelize",
                "--sort-input",
            ],
        ),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_workspace_members_run_from_module_root() {
    let dir = TestDir::new("moon_test/workspace_cwd");
    let spawn_dir = dir.join("spawn");
    std::fs::create_dir(&spawn_dir).expect("failed to create spawn directory");

    check(
        get_stdout(
            &spawn_dir,
            [
                "-C",
                "..",
                "test",
                "--target",
                "js",
                "--no-parallelize",
                "--sort-input",
            ],
        ),
        expect![[r#"
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_moon_test_resolves_wasm_policy_before_changing_to_module_roots() {
    let dir = TestDir::new("moon_test/workspace_cwd");
    let spawn_dir = dir.join("spawn");
    std::fs::create_dir(&spawn_dir).expect("failed to create spawn directory");

    moon_cmd(&spawn_dir)
        .args([
            "-C",
            "..",
            "test",
            "--target",
            "wasm",
            "--wasm-policy",
            "policy.json",
            "--no-parallelize",
            "--sort-input",
        ])
        .assert()
        .success()
        .stdout_eq("Total tests: 2, passed: 2, failed: 0.\n");
}

#[test]
fn test_zombie_child_process() {
    use super::process::{
        read_pid_file, terminate_child, terminate_pid, wait_for_child_exit, wait_for_pid_exit,
    };
    use std::process::Stdio;
    use std::thread;
    use std::time::{Duration, Instant};

    let dir = TestDir::new("moon_test/zombie_child");
    let child_pid_file = dir.join("test_child_pid.txt");

    let build_output = moon_process_cmd(&dir)
        .args(["test", "--target", "js", "--no-parallelize", "--build-only"])
        .output()
        .expect("Failed to build zombie child test fixture");
    assert!(
        build_output.status.success(),
        "failed to build zombie child test fixture\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr)
    );

    // Spawn moon test in background
    let mut moon_child = moon_process_cmd(&dir)
        .args(["test", "--target", "js", "--no-parallelize"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn moon test");

    // Wait for the test executable to publish its PID.
    let start = Instant::now();
    let child_pid = loop {
        if let Ok(pid) = read_pid_file(&child_pid_file) {
            break pid;
        }
        if let Some(status) = moon_child
            .try_wait()
            .expect("Failed to poll moon test process")
        {
            let output = moon_child
                .wait_with_output()
                .expect("Failed to collect moon test output");
            panic!(
                "moon test exited before writing child PID: {status}\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(100));
        if start.elapsed() > Duration::from_secs(60) {
            let _ = moon_child.kill();
            let output = moon_child
                .wait_with_output()
                .expect("Failed to collect moon test output");
            panic!(
                "Timeout waiting for child PID to be written\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    };

    // Terminate the moon process (simulating the scenario in example/script.js)
    terminate_child(&mut moon_child);

    // When moon is killed, all child processes (moonrun/node) should also be terminated.
    // This is verified by checking that the test executable PID exits after
    // the parent `moon` process has been terminated.
    if !wait_for_child_exit(&mut moon_child, Duration::from_secs(5)) {
        let _ = moon_child.kill();
        let _ = moon_child.wait();
        terminate_pid(child_pid);
        panic!("moon process did not exit after termination request");
    }
    if !wait_for_pid_exit(child_pid, Duration::from_secs(5)) {
        terminate_pid(child_pid);
        panic!(
            "Child processes (moonrun/node) are not terminated when moon is killed. \
        The test executable process with PID {child_pid} is still alive after timeout. \
        Moon should properly propagate termination signals to all child processes."
        );
    }
}

#[test]
fn test_moon_test_no_entry_warning() {
    let dir = TestDir::new("moon_test/no_entry_warning");

    moon_cmd(&dir)
        .args(["test"])
        .assert()
        .success()
        .stderr_eq(snapbox::str![[r#"
Warning: no test entry found.

"#]]);
}

#[test]
#[ignore]
fn test_generate_test_driver_incremental() {
    let dir = TestDir::new("moon_test/hello_lib");

    get_stdout(&dir, ["test", "--package", "moonbitlang/hello/lib"]);
    let driver_file =
        dir.join("_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt");
    assert!(driver_file.exists());

    let time_1 = driver_file.metadata().unwrap().modified().unwrap();

    get_stdout(
        &dir,
        [
            "test",
            "--package",
            "moonbitlang/hello/lib",
            "--file",
            "hello_wbtest.mbt",
        ],
    );
    let time_2 = driver_file.metadata().unwrap().modified().unwrap();

    assert!(time_1 == time_2);

    get_stdout(
        &dir,
        [
            "test",
            "--package",
            "moonbitlang/hello/lib",
            "--file",
            "hello_wbtest.mbt",
            "--index",
            "0",
        ],
    );
    let time_3 = driver_file.metadata().unwrap().modified().unwrap();

    assert!(time_2 == time_3);

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(dir.join("lib/hello.mbt"))
        .unwrap();
    file.write_all(b"\n").unwrap();

    get_stdout(
        &dir,
        [
            "test",
            "--package",
            "moonbitlang/hello/lib",
            "--file",
            "hello_wbtest.mbt",
            "--index",
            "0",
        ],
    );
    let time_4 = driver_file.metadata().unwrap().modified().unwrap();

    assert!(time_3 != time_4);
}

#[test]
fn test_async_test_inline() {
    let dir = TestDir::new("moon_test");

    let out1 = get_stdout(&dir, ["-C", "async_test_inline", "test"]);
    check(
        &out1,
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    )
}

#[test]
fn test_async_test() {
    let dir = TestDir::new("moon_test");
    let out1 = get_stdout(
        &dir,
        [
            "-C",
            "async_test",
            "test",
            "--package",
            "moon/test_async_test",
            "--file",
            "async_test.mbt",
            "--index",
            "0",
        ],
    );
    check(
        &out1,
        expect![[r#"
        Total tests: 1, passed: 1, failed: 0.
    "#]],
    );
    let out2 = get_err_stdout(
        &dir,
        [
            "-C",
            "async_test",
            "test",
            "--package",
            "moon/test_async_test",
            "--file",
            "async_test.mbt",
            "--index",
            "1",
        ],
    );
    let last_line = out2.lines().last().unwrap_or("");
    check(last_line, expect!["Total tests: 1, passed: 0, failed: 1."])
}

#[test]
fn test_max_concurrent_tests() {
    let dir = TestDir::new("moon_test");
    let out1 = get_stdout(
        &dir,
        [
            "-C",
            "max_concurrent_tests",
            "test",
            "-p",
            "moon/test_async_test/with_limit",
        ],
    );
    check(
        &out1,
        expect![[r#"
            test 1 msg 1
            test 1 msg 2
            test 2 msg 1
            test 2 msg 2
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
    let out2 = get_stdout(
        &dir,
        [
            "-C",
            "max_concurrent_tests",
            "test",
            "-p",
            "moon/test_async_test/no_limit",
        ],
    );
    check(
        &out2,
        expect![[r#"
            test 1 msg 1
            test 2 msg 1
            test 1 msg 2
            test 2 msg 2
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );
}

#[test]
fn test_doctest_without_bbtest_file() {
    let dir = TestDir::new("moon_test/doctest_without_bbtest");

    let out1 = get_stdout(&dir, ["test"]);
    check(
        &out1,
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    )
}
