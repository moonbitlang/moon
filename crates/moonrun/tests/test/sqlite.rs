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

use super::{MOONBIT_ASYNC_CHECK_FD_LEAK, TestDir, moon_cmd};

#[test]
fn sqlite_jobs_use_public_completion_pipes_from_moonbit() {
    let dir = TestDir::new("test_sqlite_async.in");
    let upstream = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/moonbitlang_async");
    // Exercise SQLite against the pinned async implementation using only its
    // public APIs, without patching its scheduler or package visibility.
    moon_test_util::test_dir::copy_tree(&upstream.join("src"), &dir.join("async/src"), false)
        .unwrap();
    std::fs::copy(upstream.join("moon.mod"), dir.join("async/moon.mod")).unwrap();
    moon_test_util::test_dir::copy_tree(&dir.join("test"), &dir.join("async/src/test"), false)
        .unwrap();
    moon_cmd()
        .current_dir(dir.join("async"))
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .args(["test", "--target", "wasm", "src/test"])
        .timeout(std::time::Duration::from_secs(60))
        .assert()
        .success()
        .stdout_eq("Total tests: 7, passed: 7, failed: 0.\n");
}

#[test]
fn test_moon_run_with_sqlite_ffi_imports() {
    let dir = TestDir::new("test_sqlite_ffi.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/main/main.wasm");
    std::fs::create_dir(dir.join("allowed")).unwrap();
    std::fs::create_dir(dir.join("denied")).unwrap();
    let policy_file = dir.join("policy.toml");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(&dir)
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .arg("--policy")
        .arg(&policy_file)
        .arg(&wasm_file)
        .assert()
        .success()
        .stdout_eq("ok\n")
        .stderr_eq(snapbox::str![[r#"
Sandbox policy blocked file read: "denied/database.sqlite"

"#]]);

    assert!(dir.join("allowed/database.sqlite").exists());
    assert!(!dir.join("denied/database.sqlite").exists());

    let leaked_wasm = dir.join("_build/wasm/debug/build/leak/leak.wasm");
    let assert = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(&dir)
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .env("RUST_BACKTRACE", "0")
        .arg(&leaked_wasm)
        .assert()
        .failure()
        .stdout_eq("leaked\n");
    let stderr = std::str::from_utf8(&assert.get_output().stderr).unwrap();
    assert!(
        stderr.contains("moonrun Runtime leaked host state: sqlite(databases=1)"),
        "expected SQLite leak assertion in stderr, got:\n{stderr}"
    );

    let invalid_handle_wasm =
        dir.join("_build/wasm/debug/build/invalid_handle/invalid_handle.wasm");
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&invalid_handle_wasm)
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: moonbitlang/sqlite.sqlite3_close failed: InvalidHandle
...
"#]]);

    let entered_mutex_wasm = dir.join("_build/wasm/debug/build/entered_mutex/entered_mutex.wasm");
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&entered_mutex_wasm)
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: moonbitlang/sqlite.sqlite3_close failed: Fault
...
"#]]);
}

#[test]
fn test_sqlite_binding_order() {
    let dir = TestDir::new("test_sqlite_ffi.in");

    moon_cmd()
        .current_dir(&dir)
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .args(["test", "main", "--target", "wasm"])
        .assert()
        .success()
        .stdout_eq("Total tests: 3, passed: 3, failed: 0.\n");
}
