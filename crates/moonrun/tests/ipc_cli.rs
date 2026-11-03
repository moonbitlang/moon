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

#![cfg(all(unix, feature = "v8"))]

use std::fs;
use std::path::Path;
use std::time::Duration;

#[test]
fn cli_reuses_the_host_through_a_native_shell() {
    let directory = tempfile::tempdir().unwrap();
    let project = directory.path().join("project");
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
    let policy = project.join("policy.toml");
    fs::write(
        &policy,
        r#"
[env]
from_host = ["*"]
[fs]
read = ["child"]
[process]
ipc = true
allow = [{ program = "/bin/sh" }]
"#,
    )
    .unwrap();
    let output = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(&project)
        .arg("--policy")
        .arg(&policy)
        .arg(&wasm)
        .arg("parent")
        .arg(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm)
        .timeout(Duration::from_secs(30))
        .assert()
        .success();
    // The shell execs the real client PID; guest getpid identifies the host.
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 4, "{stdout}");
    assert_eq!(
        lines[0].strip_prefix("host="),
        lines[2].strip_prefix("guest=")
    );
    assert_ne!(
        lines[0].strip_prefix("host="),
        lines[1].strip_prefix("client=")
    );
    assert_eq!(lines[3], "caller state and ceiling preserved");
    fs::write(
        project.join("child/narrow.toml"),
        r#"
[env]
from_host = ["*"]
[process]
allow = [{ program = "/bin/sh" }]
"#,
    )
    .unwrap();
    fs::write(
        project.join("child/permissive.toml"),
        r#"
[env]
from_host = ["*"]
[fs]
read = ["*"]
[process]
spawn = true
"#,
    )
    .unwrap();
    for (mode, expected) in [
        ("nested", "nested ceiling preserved\n"),
        ("signals", "client signals and disconnect cleaned up\n"),
    ] {
        snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
            .current_dir(&project)
            .arg("--policy")
            .arg(&policy)
            .arg(&wasm)
            .arg(mode)
            .arg(snapbox::cmd::cargo_bin!("moonrun"))
            .arg(&wasm)
            .timeout(Duration::from_secs(30))
            .assert()
            .success()
            .stdout_eq(expected);
    }
    assert_host_shutdown(&project, &policy, &wasm);
    assert_terminal_roundtrip(&project, &policy, &wasm);
    fs::write(
        &policy,
        fs::read_to_string(&policy)
            .unwrap()
            .replace("ipc = true", "ipc = false"),
    )
    .unwrap();
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(&project)
        .arg("--policy")
        .arg(&policy)
        .arg(&wasm)
        .arg("local")
        .arg(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm)
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .stdout_eq("local execution preserved\n");
}

#[test]
fn stale_session_fails_without_local_execution_but_help_stays_local() {
    let directory = tempfile::tempdir().unwrap();
    let wasm = directory.path().join("empty.wasm");
    fs::write(&wasm, b"\0asm\x01\0\0\0").unwrap();
    let endpoint = directory.path().join("missing.sock");
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .env(moonrun::ipc::ENDPOINT_ENV, &endpoint)
        .arg(&wasm)
        .timeout(Duration::from_secs(10))
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Error: failed to connect to inherited moonrun IPC session

Caused by:
    [..]
"#]]);
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .env(moonrun::ipc::ENDPOINT_ENV, endpoint)
        .arg("--help")
        .timeout(Duration::from_secs(10))
        .assert()
        .success();
}

// Every helper child has its own OS process group, used only for cleanup when
// an assertion fails. The behavior under test signals the CLI PID itself.
struct ChildGroup(std::process::Child);

impl Drop for ChildGroup {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        let _ = self.0.wait();
    }
}

fn assert_host_shutdown(project: &Path, policy: &Path, wasm: &Path) {
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    let mut child = ChildGroup(
        std::process::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
            .current_dir(project)
            .arg("--policy")
            .arg(policy)
            .arg(wasm)
            .arg("host-wait")
            .arg(snapbox::cmd::cargo_bin!("moonrun"))
            .arg(wasm)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .process_group(0)
            .spawn()
            .unwrap(),
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while fs::read(project.join("child/ready")).map_or(true, |contents| contents != b"ready\n") {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "host exited before readiness"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "delegated Run never became ready"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let endpoint = fs::read_to_string(project.join("child/endpoint")).unwrap();
    unsafe {
        assert_eq!(libc::kill(child.0.id() as i32, libc::SIGTERM), 0);
        libc::kill(child.0.id() as i32, libc::SIGTERM);
    }
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "host did not finish IPC teardown"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.signal(), Some(libc::SIGTERM));
    assert!(
        !Path::new(&endpoint).exists(),
        "IPC endpoint survived host teardown"
    );
    assert!(
        !Path::new(&endpoint).parent().unwrap().exists(),
        "IPC session directory survived"
    );
}

fn assert_terminal_roundtrip(project: &Path, policy: &Path, wasm: &Path) {
    use std::io::{Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::process::CommandExt;
    let (mut master, slave) = unsafe {
        let (mut master, mut slave) = (-1, -1);
        assert_eq!(
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut()
            ),
            0
        );
        (fs::File::from_raw_fd(master), fs::File::from_raw_fd(slave))
    };
    rustix::io::fcntl_setfd(&master, rustix::io::FdFlags::CLOEXEC).unwrap();
    rustix::io::fcntl_setfd(&slave, rustix::io::FdFlags::CLOEXEC).unwrap();
    let mut command = std::process::Command::new(snapbox::cmd::cargo_bin!("moonrun"));
    command
        .current_dir(project)
        .arg("--policy")
        .arg(policy)
        .arg(wasm)
        .arg("terminal")
        .arg(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(wasm)
        .stdin(slave.try_clone().unwrap())
        .stdout(slave.try_clone().unwrap())
        .stderr(slave);
    unsafe {
        // Only async-signal-safe system calls run between fork and exec.
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = ChildGroup(command.spawn().unwrap());
    master.write_all(b"hello\n").unwrap();
    unsafe {
        let flags = libc::fcntl(master.as_raw_fd(), libc::F_GETFL);
        assert!(flags >= 0);
        assert_eq!(
            libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK),
            0
        );
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut output = Vec::new();
    loop {
        let mut bytes = [0; 1024];
        match master.read(&mut bytes) {
            Ok(0) => break,
            Ok(count) => output.extend_from_slice(&bytes[..count]),
            Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "terminal request hung: {}",
                    String::from_utf8_lossy(&output)
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("terminal read failed: {error}"),
        }
    }
    assert!(
        child.0.wait().unwrap().success(),
        "{}",
        String::from_utf8_lossy(&output)
    );
    assert!(String::from_utf8_lossy(&output).contains("terminal roundtrip"));
}
