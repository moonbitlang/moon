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

use std::path::PathBuf;

use super::{MOONBIT_ASYNC_CHECK_FD_LEAK, TestDir, moon_cmd};

#[test]
fn test_moonrun_async_host_exit_returns_guest_exit_code() {
    let dir = TestDir::new("test_async_exit.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(dir.join("_build/wasm/debug/build/main/main.wasm"))
        .assert()
        .code(9)
        .stdout_eq("")
        .stderr_eq("");
}

#[test]
fn test_moonrun_async_host_signal_termination_is_scoped_to_wasm_run() {
    let dir = TestDir::new("test_async_signal_termination.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let output = std::process::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(dir.join("_build/wasm/debug/build/main/main.wasm"))
        .output()
        .expect("run wasm signal termination fixture");

    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(output.status.signal(), Some(libc::SIGINT));
    }
    #[cfg(windows)]
    assert_eq!(output.status.code(), Some(0xC000_013A_u32 as i32));
}

#[test]
fn test_moonrun_async_main_preserves_signal_termination() {
    use std::io::{BufRead, Read};
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    use std::sync::mpsc::{RecvTimeoutError, sync_channel};
    use std::time::{Duration, Instant};

    let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
    let case_dir = case_root.join("test_async_signal_main.in");
    let dir = tempfile::Builder::new()
        .prefix("test_async_signal_main.")
        .tempdir_in(&case_root)
        .expect("create temp fixture");
    moon_test_util::test_dir::copy_tree(&case_dir, dir.path(), false).expect("copy test fixture");

    moon_cmd()
        .current_dir(dir.path())
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir
        .path()
        .join("_build/wasm/debug/build/moon/async_signal_main/main/main.wasm");
    let mut command = std::process::Command::new(snapbox::cmd::cargo_bin!("moonrun"));
    command
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .arg(wasm_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP);
    let mut child = command.spawn().expect("start async wasm main");
    let deadline = Instant::now() + Duration::from_secs(10);
    let (ready_tx, ready_rx) = sync_channel(1);
    let stdout_thread = std::thread::spawn({
        let stdout = child.stdout.take().expect("capture stdout");
        move || {
            let mut stdout = std::io::BufReader::new(stdout);
            let mut ready = String::new();
            let result = stdout.read_line(&mut ready);
            let _ = ready_tx.send((result, ready));
            stdout
        }
    });
    let ready = match ready_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok((Ok(_), ready)) => ready,
        Ok((Err(error), _)) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            panic!("failed to read async wasm main readiness: {error}");
        }
        Err(RecvTimeoutError::Timeout) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            panic!("async wasm main did not become ready before timeout");
        }
        Err(RecvTimeoutError::Disconnected) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            panic!("async wasm main readiness reader stopped unexpectedly");
        }
    };
    let mut stdout = stdout_thread.join().expect("join readiness reader");
    if ready != "ready\n" {
        let _ = child.kill();
        let _ = child.wait();
        panic!("unexpected async wasm main readiness output: {ready:?}");
    }

    #[cfg(unix)]
    assert_eq!(
        unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGINT) },
        0
    );
    #[cfg(windows)]
    assert_ne!(
        unsafe {
            windows_sys::Win32::System::Console::GenerateConsoleCtrlEvent(
                windows_sys::Win32::System::Console::CTRL_BREAK_EVENT,
                child.id(),
            )
        },
        0
    );
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for async wasm main") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("async wasm main did not terminate after cancellation signal");
        }
        std::thread::sleep(Duration::from_millis(10));
    };

    let mut remaining_stdout = String::new();
    stdout
        .read_to_string(&mut remaining_stdout)
        .expect("read remaining stdout");
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("capture stderr")
        .read_to_string(&mut stderr)
        .expect("read stderr");
    assert_eq!(remaining_stdout, "cleanup\n");
    assert_eq!(stderr, "");
    #[cfg(unix)]
    assert_eq!(status.signal(), Some(libc::SIGINT));
    #[cfg(windows)]
    assert_eq!(status.code(), Some(0xC000_013A_u32 as i32));
}

#[test]
fn test_moon_run_with_async_host_imports() {
    let dir = TestDir::new("test_async_host.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .arg(&wasm_file)
        .assert()
        .success()
        .stdout_eq("ok\n");
}

#[test]
fn test_moon_run_async_host_leak_check_env() {
    let dir = TestDir::new("test_async_host_leak_check.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .env_remove(MOONBIT_ASYNC_CHECK_FD_LEAK)
        .arg(&wasm_file)
        .assert()
        .success()
        .stdout_eq("leaked\n")
        .stderr_eq("");

    let assert = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .env(MOONBIT_ASYNC_CHECK_FD_LEAK, "1")
        .env("RUST_BACKTRACE", "0")
        .arg(&wasm_file)
        .assert()
        .failure()
        .stdout_eq("leaked\n");
    let stderr = std::str::from_utf8(&assert.get_output().stderr).unwrap();
    assert!(
        stderr.contains("moonrun Runtime leaked host state: async(polls=1"),
        "expected async host leak assertion in stderr, got:\n{stderr}"
    );
}

#[test]
fn test_moon_run_with_async_host_invalid_c_buffer_traps() {
    let dir = TestDir::new("test_async_host_invalid_c_buffer.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: moonbitlang/async.c_buffer/c_buffer_get failed: Badf
...
"#]]);
}
