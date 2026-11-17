use super::path_with_fake_bin;
use crate::{TestDir, get_stdout_with_envs, moon_process_cmd};
use std::path::PathBuf;

#[cfg(any(unix, windows))]
#[test]
fn test_external_subcommand_delegation_handles_interrupt() {
    let dir = TestDir::new_empty();
    let fake_bin_dir = tempfile::TempDir::new().expect("failed to create fake bin dir");
    let ready_file = fake_bin_dir.path().join("moon-test-behavior-ready");
    compile_signal_fixture(fake_bin_dir.path());

    let path = path_with_fake_bin(fake_bin_dir.path());
    let mut command = interruptible_moon_command(&dir);
    let mut child = command
        .arg("test-behavior")
        .arg(&ready_file)
        .env("PATH", path)
        .spawn()
        .expect("failed to spawn moon test-behavior");

    wait_for_ready_file(&mut child, &ready_file);
    send_interrupt(child.id());
    let status = wait_for_child_status(&mut child);
    assert_eq!(
        status.code(),
        Some(42),
        "fake moon-test-behavior did not handle interrupt itself; status was {status}"
    );
}

#[test]
fn test_ide_delegation_resolves_from_change_directory() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/chdir-executable-resolution");

    let early_stdout = get_stdout_with_envs(&dir, ["-qCsubdir", "help", "ide"], [("PATH", "bin")]);
    assert!(
        early_stdout.contains("fake-moon-ide-location=subdir"),
        "early moon-ide delegation should resolve from the -C directory:\n{early_stdout}"
    );

    let parsed_stdout =
        get_stdout_with_envs(&dir, ["-C", "subdir", "ide", "--help"], [("PATH", "bin")]);
    assert!(
        parsed_stdout.contains("fake-moon-ide-location=subdir"),
        "parsed moon-ide delegation should resolve from the -C directory:\n{parsed_stdout}"
    );
}

#[test]
fn test_mooncake_direct_commands_resolve_relative_override_after_change_directory() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cram-override-cwd");

    for command in ["login", "register"] {
        let stdout = get_stdout_with_envs(
            &dir,
            ["-C", "subdir", command],
            [("MOONCAKE_OVERRIDE", "bin/fake-moon-cram")],
        );
        assert!(
            stdout.contains("fake-moon-cram-location=subdir"),
            "moon {command} should resolve mooncake from the -C directory:\n{stdout}"
        );
    }
}

#[cfg(unix)]
fn interruptible_moon_command(dir: &impl AsRef<std::path::Path>) -> std::process::Command {
    moon_process_cmd(dir)
}

#[cfg(windows)]
fn interruptible_moon_command(dir: &impl AsRef<std::path::Path>) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;

    let mut command = moon_process_cmd(dir);
    command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    command
}

#[cfg(unix)]
fn send_interrupt(pid: u32) {
    let rc = unsafe { libc::kill(pid as libc::pid_t, libc::SIGINT) };
    assert_eq!(
        rc,
        0,
        "failed to send SIGINT to delegated process: {}",
        std::io::Error::last_os_error()
    );
}

#[cfg(windows)]
fn send_interrupt(pid: u32) {
    use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, GenerateConsoleCtrlEvent};

    let ok = unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) };
    assert_ne!(
        ok,
        0,
        "failed to send CTRL_BREAK_EVENT to delegated process group: {}",
        std::io::Error::last_os_error()
    );
}

#[cfg(any(unix, windows))]
fn compile_signal_fixture(fake_bin_dir: &std::path::Path) -> PathBuf {
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/external_signal_handler.rs");
    let executable = fake_bin_dir.join(format!(
        "moon-test-behavior{}",
        std::env::consts::EXE_SUFFIX
    ));
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = std::process::Command::new(rustc)
        .arg("--edition=2021")
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .status()
        .expect("failed to compile fake moon-test-behavior");
    assert!(
        status.success(),
        "failed to compile fake moon-test-behavior"
    );
    executable
}

#[cfg(any(unix, windows))]
fn wait_for_ready_file(child: &mut std::process::Child, ready_file: &std::path::Path) {
    let start = std::time::Instant::now();
    loop {
        if ready_file.exists() {
            break;
        }
        if let Some(status) = child.try_wait().expect("failed to poll moon subprocess") {
            panic!("moon exited before fake moon-test-behavior was ready: {status}");
        }
        if start.elapsed() > std::time::Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timed out waiting for fake moon-test-behavior to be ready");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[cfg(any(unix, windows))]
fn wait_for_child_status(child: &mut std::process::Child) -> std::process::ExitStatus {
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("failed to poll moon subprocess") {
            return status;
        }
        if start.elapsed() > std::time::Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timed out waiting for fake moon-test-behavior to exit");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
