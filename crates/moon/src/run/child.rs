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

//! Handles spawning of a child process under the govern of `moon run`

use std::process::{Command, ExitStatus, Stdio};
use std::time::Instant;

use anyhow::Context;
use moonbuild::section_capture::{SectionCapture, handle_stdout_async};
use tracing::debug;
#[cfg(windows)]
use tracing::warn;

#[cfg(target_os = "macos")]
fn spawn_child(cmd: &mut tokio::process::Command) -> std::io::Result<tokio::process::Child> {
    use std::os::unix::process::CommandExt;

    struct RestoreSigmask(Option<libc::sigset_t>);

    impl RestoreSigmask {
        fn restore(mut self) -> std::io::Result<()> {
            let original = self.0.take().unwrap();
            let error = unsafe {
                libc::pthread_sigmask(libc::SIG_SETMASK, &original, std::ptr::null_mut())
            };
            if error == 0 {
                Ok(())
            } else {
                Err(std::io::Error::from_raw_os_error(error))
            }
        }
    }

    impl Drop for RestoreSigmask {
        fn drop(&mut self) {
            if let Some(original) = self.0.take() {
                // Best effort during unwinding; normal control flow reports errors.
                unsafe {
                    libc::pthread_sigmask(libc::SIG_SETMASK, &original, std::ptr::null_mut());
                }
            }
        }
    }

    let mut sigchld = unsafe { std::mem::zeroed() };
    if unsafe { libc::sigemptyset(&mut sigchld) } != 0
        || unsafe { libc::sigaddset(&mut sigchld, libc::SIGCHLD) } != 0
    {
        return Err(std::io::Error::last_os_error());
    }

    // Tokio 1.39 registers its SIGCHLD listener after spawning. This preserves
    // the historical workaround for a macOS wait hang by keeping a
    // fast-exiting child pending until that listener exists:
    // https://github.com/tokio-rs/tokio/issues/6770
    // https://github.com/tokio-rs/tokio/pull/6953
    //
    // Restore the original mask in the child before exec so user code does not
    // inherit this implementation detail. pthread_sigmask is async-signal-safe,
    // as required for the pre_exec closure.
    let mut original = unsafe { std::mem::zeroed() };
    let error = unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &sigchld, &mut original) };
    if error != 0 {
        return Err(std::io::Error::from_raw_os_error(error));
    }
    let restore_parent = RestoreSigmask(Some(original));
    let child_sigmask = original;
    unsafe {
        cmd.as_std_mut().pre_exec(move || {
            let error =
                libc::pthread_sigmask(libc::SIG_SETMASK, &child_sigmask, std::ptr::null_mut());
            if error == 0 {
                Ok(())
            } else {
                Err(std::io::Error::from_raw_os_error(error))
            }
        });
    }

    let child = cmd.spawn();
    restore_parent.restore()?;
    child
}

#[cfg(not(target_os = "macos"))]
fn spawn_child(cmd: &mut tokio::process::Command) -> std::io::Result<tokio::process::Child> {
    cmd.spawn()
}

/// Run a command under the governing of `moon run`.
///
/// If `capture` is set, the output will be captured and can be processed (like
/// `moon test`). Otherwise, the output will be directly inherited from the
/// parent process (like `moon run`).
///
/// `stdout` and `stderr` is always piped if `capture` is true, to prevent the
/// subprocess changing the property of file descriptors (`node` is notorious
/// for this, see [moon#852](https://github.com/moonbitlang/moon/issues/852)).
///
/// `captures` uses a list of [`SectionCapture`]s to capture part of the `stdout`
/// output since the running process might not have any other method to interact
/// with the host `moon` process.
pub(crate) async fn run<'a>(
    captures: &mut [&mut SectionCapture<'a>],
    capture: bool,
    cmd: Command,
) -> anyhow::Result<ExitStatus> {
    let mut cmd = tokio::process::Command::from(cmd);
    let shutdown = super::shutdown_token();
    if capture {
        // If we want to capture some/all of the output, we want to set piped
        // to both streams to prevent `node` and friends changing fd blocking
        // status
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
    } else {
        assert!(
            captures.is_empty(),
            "Can't have section captures when not capturing stdout"
        );
        // We aren't capturing, so YOLO
        cmd.stdin(Stdio::inherit());
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());
    }
    cmd.kill_on_drop(true); // to prevent zombie processes;

    let spawn_start = Instant::now();
    let mut child =
        spawn_child(&mut cmd).with_context(|| format!("Failed to spawn command {:?}", cmd))?;
    let child_pid = child.id();
    let child_start = Instant::now();
    debug!(
        child_pid = ?child_pid,
        duration_ms = spawn_start.elapsed().as_secs_f64() * 1000.0,
        "spawn_child_process_finished"
    );
    #[cfg(windows)]
    if let Err(err) = child
        .raw_handle()
        .ok_or_else(|| anyhow::anyhow!("Missing child process handle"))
        .and_then(assign_process_to_job)
    {
        warn!(?err, "Failed to assign child process to job object");
    }

    // Task only exists when capturing
    let stderr_pipe_task = child.stderr.take().map(|mut stderr| {
        tokio::spawn(async move {
            let mut proc_stderr = tokio::io::stderr();
            tokio::io::copy(&mut stderr, &mut proc_stderr)
                .await
                .context("Failed to pipe stderr to child process")
        })
    });

    // Since we cannot have scoped async tasks here, and we borrow the capture
    // sections, we'll handle stdout in this main task
    if capture {
        let child_stdout = child
            .stdout
            .take()
            .expect("Child process should have stdout piped");

        if !captures.is_empty() {
            let buf_stdout = tokio::io::BufReader::new(child_stdout);
            tokio::select! {
                res = handle_stdout_async(buf_stdout, captures) => res?,
                _ = shutdown.cancelled() => {}
            }
        } else {
            let mut child_stdout = child_stdout;
            let mut proc_stdout = tokio::io::stdout();
            tokio::select! {
                res = tokio::io::copy(&mut child_stdout, &mut proc_stdout) => { res?; }
                _ = shutdown.cancelled() => {}
            }
        }
    }

    // Wait for the child process to finish
    let post_stdout_wait_start = Instant::now();
    let mut killed = false;
    let status = tokio::select! {
        res = child.wait() => res,
        _ = shutdown.cancelled() => {
            killed = true;
            let _ = child.start_kill();
            child.wait().await
        }
    };
    debug!(
        child_pid = ?child_pid,
        killed = killed,
        duration_ms = child_start.elapsed().as_secs_f64() * 1000.0,
        post_stdout_wait_duration_ms = post_stdout_wait_start.elapsed().as_secs_f64() * 1000.0,
        "child_process_finished"
    );
    let status = status.context("Failed to wait for child process")?;

    if let Some(task) = stderr_pipe_task {
        task.await
            .expect("Failed to pipe stderr to child process")?;
    }

    Ok(status)
}

#[cfg(windows)]
pub(crate) fn assign_process_to_job(
    proc_handle: std::os::windows::io::RawHandle,
) -> anyhow::Result<()> {
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    #[derive(Clone, Copy)]
    struct JobHandle(HANDLE);
    unsafe impl Send for JobHandle {}
    unsafe impl Sync for JobHandle {}

    // Intentionally never dropped: we rely on the OS closing the last handle
    // when the parent process exits so that JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
    // can terminate any remaining child processes.
    static JOB_OBJECT: OnceLock<Result<JobHandle, std::io::Error>> = OnceLock::new();

    let job = match JOB_OBJECT.get_or_init(|| unsafe {
        let handle = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *mut _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(JobHandle(handle as HANDLE))
    }) {
        Ok(handle) => *handle,
        Err(err) => {
            return Err(anyhow::Error::new(err)).context("Failed to initialize job object");
        }
    };

    let job_handle = job.0;
    let ok = unsafe { AssignProcessToJobObject(job_handle, proc_handle) };
    if ok == 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) {
            warn!(
                ?err,
                "AssignProcessToJobObject denied; child may outlive parent"
            );
            return Ok(());
        }
        warn!(?err, "AssignProcessToJobObject failed");
        return Err(err).context("AssignProcessToJobObject failed");
    }
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::process::Stdio;
    use std::time::Duration;

    use super::spawn_child;

    const SIGCHLD_GRANDCHILD_HELPER: &str = "MOON_SIGCHLD_GRANDCHILD_HELPER";

    #[test]
    fn spawned_child_can_asynchronously_wait_for_its_child() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        if std::env::var_os(SIGCHLD_GRANDCHILD_HELPER).is_some() {
            runtime.block_on(async {
                let mut command = tokio::process::Command::new("/bin/sh");
                command.args(["-c", "sleep 0.1"]);
                let status = tokio::select! {
                    biased;
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {
                        panic!("waiting for grandchild timed out");
                    }
                    status = command.status() => status.unwrap(),
                };
                assert!(status.success());
            });
            return;
        }

        runtime.block_on(async {
            let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
            command
                .arg("spawned_child_can_asynchronously_wait_for_its_child")
                .arg("--nocapture")
                .env(SIGCHLD_GRANDCHILD_HELPER, "1")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);

            let child = spawn_child(&mut command).unwrap();
            let output = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output())
                .await
                .expect("child test process did not exit")
                .unwrap();
            assert!(
                output.status.success(),
                "child test process failed:\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        });
    }
}
