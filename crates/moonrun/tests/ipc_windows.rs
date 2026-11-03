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

#[path = "common/moonx_ipc.rs"]
mod moonx_ipc;

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
    fs::write(
        project.join("child/host-wait.cmd"),
        "@echo %MOONRUN_IPC_ENDPOINT% > endpoint\r\n@\"%IPC_RUNNER%\" \"%IPC_PROGRAM%\" spin > ready\r\n",
    ).unwrap();
    let policy = project.join("policy.toml");
    let source = r#"
[env]
from_host = ["*"]
[fs]
read = ["child"]
[process]
ipc = true
spawn = true
"#;
    fs::write(&policy, source).unwrap();
    for (mode, expected) in [
        ("windows-parent", "caller state and ceiling preserved\n"),
        ("windows-nested", "nested ceiling preserved\n"),
        ("windows-controls", "client controls preserved\n"),
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
            .env("IPC_HELPER", std::env::current_exe().unwrap())
            .env("IPC_POLICY", &policy)
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
    moonx_ipc::assert_execution_options(&project, &wasm, &policy);
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

// Run by the guest as an ordinary native descendant. A private console permits
// targeted CTRL_BREAK without delivering it to the test runner or execution host.
#[test]
fn native_client_control_helper() {
    if std::env::var_os("MOONRUN_WINDOWS_CLIENT_HELPER").is_none() {
        return;
    }
    use std::io::{BufRead, BufReader};
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use windows_sys::Win32::System::{Console::*, Threading::CREATE_NEW_PROCESS_GROUP};
    unsafe {
        FreeConsole();
    }
    assert_ne!(unsafe { AllocConsole() }, 0);
    for forced in [false, true] {
        let mut child = Command::new(std::env::var_os("IPC_RUNNER").unwrap())
            .arg(std::env::var_os("IPC_PROGRAM").unwrap())
            .arg("spin")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .unwrap();
        let mut ready = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        ready.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "ready");
        if forced {
            child.kill().unwrap();
        } else {
            assert_ne!(
                unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, child.id()) },
                0
            );
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.stderr.is_empty(),
            "{:?}",
            String::from_utf8_lossy(&output.stderr)
        );
        if forced {
            assert!(!output.status.success());
        } else {
            assert_eq!(output.status.code(), Some(0xc000013au32 as i32));
        }
    }

    // This new host has its own session and process group. Its root accepts
    // cooperative cancellation but never finishes guest cleanup. CTRL_BREAK
    // must still cancel all Runs and close the endpoint before the CLI exits.
    let ready = Path::new("child/ready");
    let _ = fs::remove_file(ready);
    let mut host = Command::new(std::env::var_os("IPC_RUNNER").unwrap())
        .env_remove("MOONRUN_IPC_ENDPOINT")
        .arg("--policy")
        .arg(std::env::var_os("IPC_POLICY").unwrap())
        .arg(std::env::var_os("IPC_PROGRAM").unwrap())
        .arg("windows-host-wait")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .unwrap();
    let result = (|| -> std::io::Result<()> {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while fs::read(ready).map_or(true, |bytes| bytes != b"ready\n") {
            if host.try_wait()?.is_some() || std::time::Instant::now() >= deadline {
                return Err(std::io::Error::other("IPC host did not become ready"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let endpoint = fs::read_to_string("child/endpoint")?;
        if unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, host.id()) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let status = loop {
            if let Some(status) = host.try_wait()? {
                break status;
            }
            if std::time::Instant::now() >= deadline {
                return Err(std::io::Error::other("IPC host did not finish shutdown"));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(status.code(), Some(0xc000013au32 as i32));
        assert!(moonrun::ipc::Connection::connect(endpoint.trim()).is_err());
        Ok(())
    })();
    if result.is_err() {
        let _ = host.kill();
    }
    let _ = host.wait();
    result.unwrap();
}
