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

//! Native process workers: materialize authority transfer, retain Resource
//! owners, bracket cancellation, and publish results through the async contract.

use super::os::Executable;
use super::{SpawnOptions, os};
use crate::async_host::{AsyncHostError, AsyncHostResult};
#[cfg(unix)]
use crate::async_sys::internal::event_loop::thread_pool::with_cancellable_region;
use crate::async_sys::ported_fns;
use crate::policy::Policy;
#[cfg(any(windows, target_os = "linux"))]
use crate::resource::Resource;
use crate::resource::{ResourcePublication, ResourceRef};
use std::ffi::{OsStr, OsString};
#[cfg(target_os = "linux")]
use std::os::fd::IntoRawFd;
#[cfg(unix)]
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, AsRawSocket, IntoRawHandle};

ported_fns! {
    #[ported(runtime = "thread_pool.c", original = "spawn_job_worker")]
    #[allow(clippy::too_many_arguments)]
    #[cfg(unix)]
    pub(super) fn run_spawn_job_unix(
        executable: Executable,
        args: Vec<OsString>,
        mut env: Vec<OsString>,
        stdio: [Option<ResourceRef>; 3],
        cwd: Option<OsString>,
        options: SpawnOptions,
        policy_inheritance: Option<Policy>,
        result: &mut Option<ResourcePublication>,
    ) -> AsyncHostResult<i64> {
        // Transfer creation belongs to worker execution: its temporary-storage
        // errors must be reported before creating an OS child.
        let transfer = policy_inheritance
            .map(|policy| policy.open_transfer())
            .transpose()
            .map_err(policy_transfer_error)?;
        let transfer_fd = transfer.as_ref().map(|file| {
            // SAFETY: PolicyTransfer owns this open fd and stays alive
            // through spawn. Its public interface exposes only AsRawFd.
            unsafe { BorrowedFd::borrow_raw(file.as_raw_fd()) }
        });
        let inherited = transfer_fd.map(os::duplicate_inherited_file).transpose()?;
        if let Some(file) = &inherited {
            crate::async_sys::process::overwrite_process_env_var(
                &mut env,
                OsStr::new(moonutil::constants::MOONRUN_INHERITED_POLICY),
                OsStr::new(&file.as_raw_fd().to_string()),
            );
        }
        let mut files = [None; 3];
        for (slot, resource) in files.iter_mut().zip(&stdio) {
            *slot = resource
                .as_ref()
                .map(|resource| resource.as_file().map(|file| file.as_raw_fd()))
                .transpose()?;
        }
        let child = os::spawn(os::Spawn {
            executable,
            args,
            env,
            stdio: files,
            cwd,
            child_signal_mask: options.child_signal_mask,
            inherited: transfer_fd.zip(inherited.as_ref().map(AsFd::as_fd)),
        })?;
        #[cfg(target_os = "linux")]
        if let Some(handle) = child.handle {
            *result = Some(ResourcePublication::Unpublished(Resource::new(
                handle.into_raw_fd(),
            )));
        }
        #[cfg(not(target_os = "linux"))]
        let _ = result;
        Ok(i64::from(child.pid))
    }

    #[ported(runtime = "thread_pool.c", original = "spawn_job_worker")]
    #[allow(clippy::too_many_arguments)]
    #[cfg(windows)]
    pub(super) fn run_spawn_job_windows(
        executable: Executable,
        command_line: OsString,
        mut env: Vec<u16>,
        stdio: [Option<ResourceRef>; 3],
        cwd: Option<OsString>,
        options: SpawnOptions,
        policy_inheritance: Option<Policy>,
        result: &mut Option<ResourcePublication>,
    ) -> AsyncHostResult<i64> {
        let transfer = policy_inheritance
            .map(|policy| policy.open_transfer())
            .transpose()
            .map_err(policy_transfer_error)?;
        let inherited = transfer
            .as_ref()
            .map(|file| os::duplicate_inheritable_handle(file.as_raw_handle()))
            .transpose()?;
        if let Some(handle) = &inherited {
            crate::async_sys::process::overwrite_process_env_var(
                &mut env,
                OsStr::new(moonutil::constants::MOONRUN_INHERITED_POLICY),
                OsStr::new(&(handle.as_raw_handle() as usize).to_string()),
            );
        }
        let mut handles = [None; 3];
        for (slot, resource) in handles.iter_mut().zip(&stdio) {
            *slot = resource.as_deref().map(spawn_stdio_handle).transpose()?;
        }
        let child = os::spawn(
            executable,
            command_line,
            env,
            handles,
            cwd,
            options,
            inherited.as_ref().map(AsRawHandle::as_raw_handle),
        )?;
        *result = Some(ResourcePublication::Unpublished(Resource::new(
            child.handle.into_raw_handle(),
        )));
        Ok(i64::from(child.pid))
    }

    #[ported(runtime = "thread_pool.c", original = "wait_for_process_job_worker")]
    pub(super) fn run_wait_for_process_job(
        handle: Option<ResourceRef>,
        pid: i32,
        #[cfg(unix)] defer_reap: bool,
        #[cfg(windows)] cancel: Option<ResourceRef>,
    ) -> AsyncHostResult<i64> {
        #[cfg(unix)]
        {
            #[cfg(target_os = "linux")]
            if let Some(handle) = &handle {
                let pidfd = handle.as_fd()?.as_raw_fd();
                return with_cancellable_region(|| {
                    os::wait_pidfd(pidfd, defer_reap)
                        .map(exit_code)
                        .map_err(Into::into)
                });
            }
            #[cfg(not(target_os = "linux"))]
            let _ = handle;
            with_cancellable_region(|| {
                os::wait_pid(pid, defer_reap)
                    .map(exit_code)
                    .map_err(Into::into)
            })
        }
        #[cfg(windows)]
        {
            let cancel = cancel.ok_or(AsyncHostError::Badf)?;
            let cancel = cancel.as_handle()?.as_raw_handle();
            if let Some(handle) = &handle {
                return os::wait_handle(handle.as_handle()?.as_raw_handle(), cancel)
                    .map(exit_code)
                    .map_err(Into::into);
            }
            let handle = os::open_process_for_wait(pid as u32)?;
            os::wait_handle(handle.as_raw_handle(), cancel)
                .map(exit_code)
                .map_err(Into::into)
        }
    }
}

pub(crate) fn exit_code(status: std::process::ExitStatus) -> i64 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status
            .code()
            .map_or_else(|| -i64::from(status.signal().unwrap()), i64::from)
    }
    #[cfg(windows)]
    {
        i64::from(status.code().unwrap() as u32)
    }
}

#[cfg(windows)]
fn spawn_stdio_handle(resource: &Resource) -> AsyncHostResult<os::RawFile> {
    // Sockets stay typed Resources until this native spawn conversion.
    if resource.resource_class().is_socket() {
        Ok(resource.as_socket()?.as_raw_socket() as os::RawFile)
    } else {
        Ok(resource.as_file()?.as_raw_handle())
    }
}

#[cfg(windows)]
pub(super) fn make_wait_for_process_cancel() -> AsyncHostResult<Resource> {
    Ok(Resource::new(
        os::make_wait_for_process_cancel()?.into_raw_handle(),
    ))
}

#[cfg(windows)]
pub(super) fn cancel_wait_for_process(cancel: &ResourceRef) -> AsyncHostResult<()> {
    os::cancel_wait_for_process(cancel.as_handle()?.as_raw_handle()).map_err(Into::into)
}

fn policy_transfer_error(error: std::io::Error) -> AsyncHostError {
    error
        .raw_os_error()
        .map_or(AsyncHostError::Io, AsyncHostError::Native)
}

#[cfg(all(test, unix))]
fn last_native_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap()
}

#[cfg(test)]
mod policy_transfer_error_tests {
    use super::*;

    #[test]
    fn preserves_native_errors_for_the_spawn_job() {
        assert_eq!(
            policy_transfer_error(std::io::Error::from_raw_os_error(1234)),
            AsyncHostError::Native(1234)
        );
    }

    #[test]
    fn reports_non_native_transfer_failures_as_io() {
        assert_eq!(
            policy_transfer_error(std::io::Error::other("policy transfer failed")),
            AsyncHostError::Io
        );
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    fn prepared(program: &OsStr, path: Option<&OsStr>, cwd: &Path) -> Executable {
        Executable::Prepared(super::super::lookup::unix(program, path, Some(cwd)).unwrap())
    }

    fn spawn_for_test(
        executable: Executable,
        args: Vec<OsString>,
        cwd: &Path,
    ) -> AsyncHostResult<i64> {
        let mut child_signal_mask = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::sigemptyset(&mut child_signal_mask) }, 0);
        run_spawn_job_unix(
            executable,
            args,
            vec!["PATH=/not-the-frozen-search-path".into()],
            [None, None, None],
            Some(cwd.as_os_str().to_owned()),
            SpawnOptions { child_signal_mask },
            None,
            &mut None,
        )
    }

    #[test]
    fn frozen_search_retries_after_an_unavailable_interpreter() {
        #[cfg(target_os = "linux")]
        {
            if std::env::var_os("MOONRUN_SINGLE_FORK_SEARCH").is_none() {
                // atfork callbacks cannot be unregistered. Keep the process
                // counter isolated from unrelated tests and their children.
                snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "process::native::tests::frozen_search_retries_after_an_unavailable_interpreter",
                    ])
                    .env("MOONRUN_SINGLE_FORK_SEARCH", "1")
                    .timeout(std::time::Duration::from_secs(15))
                    .assert()
                    .success();
                return;
            }
        }
        #[cfg(target_os = "linux")]
        static FORKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        #[cfg(target_os = "linux")]
        unsafe extern "C" fn count_fork() {
            FORKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        let path = std::env::join_paths([&first, &second]).unwrap();
        // Freeze lookup before either executable exists. Only execution can
        // decide that the first image has no usable interpreter.
        let executable = prepared(OsStr::new("program"), Some(&path), directory.path());
        for (target, contents) in [
            (
                first.join("program"),
                "#!/moonrun-missing-interpreter\nexit 99\n",
            ),
            (second.join("program"), "#!/bin/sh\nexit 23\n"),
        ] {
            std::fs::write(&target, contents).unwrap();
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        #[cfg(target_os = "linux")]
        assert_eq!(
            unsafe { libc::pthread_atfork(None, Some(count_fork), None) },
            0
        );
        let pid = spawn_for_test(executable, vec!["program".into()], directory.path()).unwrap();
        assert_eq!(run_wait_for_process_job(None, pid as i32, false), Ok(23));
        #[cfg(target_os = "linux")]
        assert_eq!(FORKS.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn search_preserves_permission_errors_and_direct_paths_do_not_search() {
        let directory = tempfile::tempdir().unwrap();
        let denied = directory.path().join("program");
        std::fs::write(&denied, "#!/bin/sh\nexit 99\n").unwrap();
        std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o644)).unwrap();
        let path =
            std::env::join_paths([directory.path(), &directory.path().join("missing")]).unwrap();
        let executable = prepared(OsStr::new("program"), Some(&path), directory.path());
        assert_eq!(
            spawn_for_test(executable, vec!["program".into()], directory.path()),
            Err(AsyncHostError::Native(libc::EACCES))
        );
        let executable = prepared(
            OsStr::new("missing/program"),
            Some(directory.path().as_os_str()),
            directory.path(),
        );
        assert_eq!(
            spawn_for_test(executable, vec!["program".into()], directory.path()),
            Err(AsyncHostError::Native(libc::ENOENT))
        );
    }

    #[test]
    fn executable_text_keeps_the_platform_search_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("program");
        std::fs::write(&script, "exit \"$1\"\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        for program in ["./program", "program"] {
            let executable = prepared(
                OsStr::new(program),
                Some(directory.path().as_os_str()),
                directory.path(),
            );
            let result = spawn_for_test(
                executable,
                vec!["custom-argv-zero".into(), "23".into()],
                directory.path(),
            );
            if cfg!(target_os = "macos") && program == "program" {
                assert_eq!(
                    run_wait_for_process_job(None, result.unwrap() as i32, false),
                    Ok(23)
                );
            } else {
                assert_eq!(result, Err(AsyncHostError::Native(libc::ENOEXEC)));
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn wait_for_unknown_process_reports_native_error() {
        // The current process is not its own child and cannot consume a child
        // spawned by another test, unlike the `-1` waitpid wildcard.
        let non_child_pid = unsafe { libc::getpid() };
        let err = run_wait_for_process_job(None, non_child_pid, false).unwrap_err();
        assert_eq!(err, AsyncHostError::Native(libc::ECHILD));
    }

    #[test]
    fn wait_for_process_fallback_returns_negative_signal() {
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0, "fork failed: {}", last_native_errno());
        if pid == 0 {
            unsafe {
                libc::raise(libc::SIGTERM);
                libc::_exit(1);
            }
        }

        assert_eq!(
            run_wait_for_process_job(None, pid, false),
            Ok(-i64::from(libc::SIGTERM))
        );
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn ambient_spawn_preserves_native_string_handling() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{
            CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, GetExitCodeProcess,
            PROCESS_INFORMATION, STARTUPINFOW, WaitForSingleObject,
        };

        let directory = tempfile::tempdir().unwrap();
        let command = format!("\"{}\" /d /c exit 23", std::env::var("COMSPEC").unwrap());
        let mut terminated_cwd = directory.path().as_os_str().to_owned();
        terminated_cwd.push("\0ignored");
        for (command_line, cwd) in [
            (OsString::new(), None),
            (OsString::from(format!("{command}\0ignored")), None),
            (OsString::from(&command), Some(terminated_cwd)),
            (OsString::from(&command), Some(OsString::new())),
        ] {
            let mut native_command = command_line.encode_wide().chain([0]).collect::<Vec<_>>();
            let native_cwd = cwd
                .as_ref()
                .map(|cwd| cwd.encode_wide().chain([0]).collect::<Vec<_>>());
            let env = [0u16, 0];
            let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
            startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            let mut native: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
            let created = unsafe {
                CreateProcessW(
                    std::ptr::null(),
                    native_command.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
                    env.as_ptr().cast(),
                    native_cwd
                        .as_ref()
                        .map_or(std::ptr::null(), |cwd| cwd.as_ptr()),
                    &startup,
                    &mut native,
                )
            };
            let expected = if created == 0 {
                Err(AsyncHostError::Native(unsafe { GetLastError() } as i32))
            } else {
                assert_eq!(
                    unsafe { WaitForSingleObject(native.hProcess, 10_000) },
                    WAIT_OBJECT_0
                );
                let mut exit = 0;
                assert_ne!(unsafe { GetExitCodeProcess(native.hProcess, &mut exit) }, 0);
                unsafe {
                    CloseHandle(native.hThread);
                    CloseHandle(native.hProcess);
                }
                Ok(exit)
            };
            let mut result = None;
            let actual = run_spawn_job_windows(
                Executable::Ambient,
                command_line,
                vec![0, 0],
                [None, None, None],
                cwd,
                SpawnOptions {
                    no_console_window: true,
                    is_orphan: true,
                },
                None,
                &mut result,
            )
            .map(|_| {
                let Some(ResourcePublication::Unpublished(process)) = result else {
                    panic!("successful spawn publishes a process handle");
                };
                let handle = process.as_file().unwrap().as_raw_handle();
                assert_eq!(
                    unsafe { WaitForSingleObject(handle, 10_000) },
                    WAIT_OBJECT_0
                );
                let mut exit = 0;
                assert_ne!(unsafe { GetExitCodeProcess(handle, &mut exit) }, 0);
                exit
            });
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn spawn_without_standard_handles() {
        use std::os::windows::io::AsRawHandle;
        use std::sync::Arc;
        use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Console::{
            STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
        };
        use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};

        const CHILD: &str = "MOONRUN_TEST_ABSENT_STDIO";
        if std::env::var_os(CHILD).is_none() {
            // Changing process stdio must not affect other tests or libtest's
            // own output. Exercise both Win32 representations in isolated runs.
            for missing in ["null", "invalid"] {
                assert!(
                    std::process::Command::new(std::env::current_exe().unwrap())
                        .args([
                            "--exact",
                            "process::native::windows_tests::spawn_without_standard_handles"
                        ])
                        .env(CHILD, missing)
                        .status()
                        .unwrap()
                        .success()
                );
            }
            return;
        }
        let missing = if std::env::var(CHILD).unwrap() == "null" {
            std::ptr::null_mut()
        } else {
            INVALID_HANDLE_VALUE
        };
        for stream in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            assert_ne!(unsafe { SetStdHandle(stream, missing) }, 0);
        }
        let process = super::super::HostProcess::new(
            None,
            Arc::new(crate::WorkingDirectory::Ambient),
            Arc::new(crate::runtime::Stdio::Ambient),
            Default::default(),
            Arc::new(crate::runtime::Env::ambient()),
        );

        // Cover an empty inheritance list and a partial stdio set, with and
        // without the child Job Object attribute.
        for is_orphan in [false, true] {
            for redirect_stdout in [false, true] {
                let output = tempfile::tempfile().unwrap();
                let stdout = redirect_stdout
                    .then(|| std::sync::Arc::new(Resource::stdio_file(output.as_raw_handle())));
                let command = std::env::var("COMSPEC").unwrap();
                let mut job = super::super::Job::spawn_windows(
                    format!("\"{command}\" /d /c exit 23").into(),
                    vec![0, 0],
                    None,
                    stdout,
                    None,
                    None,
                    SpawnOptions {
                        no_console_window: true,
                        is_orphan,
                    },
                );
                process.prepare_job(&mut job).unwrap();
                assert!(job.run().unwrap() > 0);
                let Some(ResourcePublication::Unpublished(process)) =
                    job.take_spawn_result().unwrap()
                else {
                    panic!("spawn must publish its process handle");
                };
                let handle = process.as_file().unwrap().as_raw_handle();
                assert_eq!(
                    unsafe { WaitForSingleObject(handle, 10_000) },
                    WAIT_OBJECT_0
                );
                let mut exit_code = 0;
                assert_ne!(unsafe { GetExitCodeProcess(handle, &mut exit_code) }, 0);
                assert_eq!(exit_code, 23);
            }
        }
    }

    #[test]
    fn spawn_stdio_accepts_socket_resource() {
        assert_eq!(crate::async_sys::internal::event_loop::io::init_wsa(), 0);
        let raw_socket = crate::async_sys::socket::make_tcp_socket(4).unwrap();
        let resource = Resource::tcp_socket(raw_socket, 4);

        assert_eq!(
            spawn_stdio_handle(&resource).unwrap() as usize,
            raw_socket as usize
        );

        drop(resource);
        assert_eq!(crate::async_sys::internal::event_loop::io::cleanup_wsa(), 0);
    }
}
