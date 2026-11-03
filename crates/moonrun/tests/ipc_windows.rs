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

#![cfg(all(windows, feature = "v8"))]

use std::{fs, path::Path, time::Duration};

#[test]
fn cli_delegates_through_cmd_and_preserves_caller_state() {
    let directory = tempfile::tempdir().unwrap();
    let project = directory.path().join("project with spaces");
    let upstream =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/moonbitlang_async");
    moon_test_util::test_dir::copy_tree(&upstream.join("src"), &project.join("src"), false)
        .unwrap();
    fs::copy(upstream.join("moon.mod"), project.join("moon.mod")).unwrap();
    moon_test_util::test_dir::copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases/test_ipc_cli.in"),
        &project.join("src/ipc_guest"),
        false,
    )
    .unwrap();
    snapbox::cmd::Command::new("moon")
        .current_dir(&project)
        .args(["build", "--target", "wasm", "src/ipc_guest"])
        .timeout(Duration::from_secs(90))
        .assert()
        .success();
    let wasm = project.join("_build/wasm/debug/build/ipc_guest/ipc_guest.wasm");
    fs::create_dir(project.join("child")).unwrap();
    fs::write(project.join("child/input"), "caller cwd").unwrap();
    fs::write(project.join("secret"), "must remain inaccessible").unwrap();
    fs::write(
        project.join("child/invoke.cmd"),
        "@\"%IPC_RUNNER%\" \"%IPC_PROGRAM%\" child \"arg with spaces\"\r\n",
    )
    .unwrap();
    fs::write(
        project.join("child/probe.cmd"),
        "@\"%IPC_RUNNER%\" \"%IPC_PROGRAM%\" probe\r\n",
    )
    .unwrap();
    let policy = project.join("policy.toml");
    let source = r#"
[env]
from_host = ["*"]
[fs]
read = ["child"]
[process]
ipc = true
allow = [{ program = "cmd.exe" }]
"#;
    fs::write(&policy, source).unwrap();
    for (mode, expected) in [
        ("windows-parent", "caller state and ceiling preserved\n"),
        ("windows-nested", "nested ceiling preserved\n"),
    ] {
        fs::write(
            project.join("child/narrow.toml"),
            "[env]\nfrom_host=[\"*\"]\n[process]\nallow=[{program=\"cmd.exe\"}]\n",
        )
        .unwrap();
        fs::write(
            project.join("child/permissive.toml"),
            "[env]\nfrom_host=[\"*\"]\n[fs]\nread=[\"*\"]\n",
        )
        .unwrap();
        fs::write(
            project.join("child/narrow.cmd"),
            "@\"%IPC_RUNNER%\" --policy narrow.toml \"%IPC_PROGRAM%\" windows-narrow\r\n",
        )
        .unwrap();
        fs::write(
            project.join("child/leaf.cmd"),
            "@\"%IPC_RUNNER%\" --policy permissive.toml \"%IPC_PROGRAM%\" leaf-denied\r\n",
        )
        .unwrap();
        snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
            .current_dir(&project)
            .env("IPC_RUNNER", snapbox::cmd::cargo_bin!("moonrun"))
            .env("IPC_PROGRAM", &wasm)
            .args(["--policy"])
            .arg(&policy)
            .arg(&wasm)
            .arg(mode)
            .timeout(Duration::from_secs(30))
            .assert()
            .success()
            .stdout_eq(expected);
    }
    fs::write(&policy, source.replace("ipc = true", "ipc = false")).unwrap();
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(&project)
        .env("IPC_RUNNER", snapbox::cmd::cargo_bin!("moonrun"))
        .env("IPC_PROGRAM", &wasm)
        .arg("--policy")
        .arg(&policy)
        .arg(&wasm)
        .arg("windows-local")
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .stdout_eq("local execution preserved\n");
}

#[test]
fn stale_endpoint_does_not_fall_back_to_local_execution() {
    let directory = tempfile::tempdir().unwrap();
    let wasm = directory.path().join("empty.wasm");
    fs::write(&wasm, b"\0asm\x01\0\0\0").unwrap();
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .env("MOONRUN_IPC_ENDPOINT", r"\\.\pipe\moonrun-missing-session")
        .arg(&wasm)
        .timeout(Duration::from_secs(10))
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: failed to connect to inherited moonrun IPC session

Caused by:
    [..]
"#]]);
}
