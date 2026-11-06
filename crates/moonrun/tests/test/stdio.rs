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

use std::time::Duration;

use super::{MOONBIT_ASYNC_CHECK_FD_LEAK, TestDir, moon_cmd};

#[test]
fn test_moon_run_with_read_bytes_from_stdin() {
    let dir = TestDir::new("test_read_bytes.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();

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
fn test_moon_run_with_core_async_stdio() {
    let dir = TestDir::new("test_async_stdio.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/main/main.wasm");

    let policy = dir.join("deny-all.json");
    std::fs::write(&policy, "{}").unwrap();
    // Compile the toolchain's public stdio implementation so changes to its
    // initialization imports are exercised without updating the async submodule.
    for policy_args in [vec![], vec!["--policy".as_ref(), policy.as_os_str()]] {
        snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
            .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
            .args(policy_args)
            .arg(&wasm_file)
            .stdin("stdio input\n")
            .timeout(Duration::from_secs(30))
            .assert()
            .success()
            .stdout_eq("stdout-ok\n")
            .stderr_eq("stderr-ok\n");
    }

    // Explicit file bindings must also initialize under a policy that denies
    // their paths. Pipe-only coverage misses this distinct stdio classification.
    let input = dir.join("stdin");
    let output = dir.join("stdout");
    let error = dir.join("stderr");
    std::fs::write(&input, "stdio input\n").unwrap();
    let options = moonrun::RunOptions::default()
        .with_policy_file(&policy)
        .unwrap()
        .with_stdio(
            std::fs::File::open(input).unwrap(),
            std::fs::File::create(&output).unwrap(),
            std::fs::File::create(&error).unwrap(),
        );
    assert_eq!(
        moonrun::Engine::default()
            .run_file(&wasm_file, options)
            .unwrap(),
        moonrun::RunOutcome::Completed,
    );
    assert_eq!(std::fs::read_to_string(output).unwrap(), "stdout-ok\n");
    assert_eq!(std::fs::read_to_string(error).unwrap(), "stderr-ok\n");
}
