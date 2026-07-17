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

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::ported_fns;

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/process.c",
        original = "moonbitlang_async_open_pid_handle"
    )]
    #[cfg(target_os = "linux")]
    pub(crate) fn open_pid_handle(pid: i32) -> AsyncHostResult<crate::async_sys::internal::fd_util::stub::RawFd> {
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
        if fd < 0 {
            Err(last_native_error())
        } else {
            Ok(fd as crate::async_sys::internal::fd_util::stub::RawFd)
        }
    }

    #[ported(
        source = "src/internal/event_loop/process.c",
        original = "moonbitlang_async_open_pid_handle"
    )]
    #[cfg(windows)]
    pub(crate) fn open_pid_handle(pid: i32) -> AsyncHostResult<crate::async_sys::internal::fd_util::stub::RawFd> {
        use windows_sys::Win32::Storage::FileSystem::SYNCHRONIZE;
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

        let handle = unsafe {
            OpenProcess(
                SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                pid as u32,
            )
        };
        if handle.is_null() {
            Err(last_native_error())
        } else {
            Ok(handle)
        }
    }

    #[ported(
        source = "src/internal/event_loop/process.c",
        original = "moonbitlang_async_get_process_result"
    )]
    pub(crate) fn get_process_result(
        handle: Option<crate::async_sys::internal::fd_util::stub::RawFd>,
        pid: i32,
    ) -> AsyncHostResult<i32> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{
                ERROR_IO_PENDING, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
            };
            use windows_sys::Win32::System::Threading::{
                GetExitCodeProcess, WaitForSingleObject,
            };

            let _ = pid;
            let handle = handle.ok_or(AsyncHostError::Badf)?;

            // Native async only calls this after its wait job completes, but a
            // Wasm guest can call the import directly. Check the waitable handle
            // because STILL_ACTIVE (259) can also be a real process exit code.
            match unsafe { WaitForSingleObject(handle, 0) } {
                WAIT_OBJECT_0 => {}
                WAIT_TIMEOUT => {
                    return Err(AsyncHostError::Native(ERROR_IO_PENDING as i32));
                }
                WAIT_FAILED => return Err(last_native_error()),
                _ => return Err(AsyncHostError::Inval),
            }

            let mut code = 0;
            if unsafe { GetExitCodeProcess(handle, &mut code) } == 0 {
                return Err(last_native_error());
            }
            Ok(code as i32)
        }

        #[cfg(unix)]
        {
            #[cfg(not(target_os = "linux"))]
            let _ = handle;

            #[cfg(target_os = "linux")]
            if let Some(handle) = handle {
                let mut info = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
                if unsafe {
                    libc::waitid(
                        libc::P_PIDFD,
                        handle as libc::id_t,
                        &mut info,
                        libc::WEXITED | libc::WNOHANG,
                    )
                } < 0
                {
                    return Err(last_native_error());
                }
                if unsafe { info.si_pid() } == 0 {
                    return Err(AsyncHostError::Native(libc::EAGAIN));
                }
                return Ok(unsafe { info.si_status() });
            }

            let mut status = 0;
            let ret = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
            if ret < 0 {
                return Err(last_native_error());
            }
            if ret == 0 {
                return Err(AsyncHostError::Native(libc::EAGAIN));
            }
            Ok(libc::WEXITSTATUS(status))
        }
    }

    #[ported(
        source = "src/process/unix.c",
        original = "moonbitlang_async_terminate_process"
    )]
    #[cfg(unix)]
    pub(crate) fn terminate_process(pid: i32, signal: i32) -> AsyncHostResult<()> {
        if unsafe { libc::kill(pid, signal) } < 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/process/unix.c",
        original = "moonbitlang_async_kill_process"
    )]
    #[cfg(unix)]
    pub(crate) fn kill_process(pid: i32) -> AsyncHostResult<()> {
        if unsafe { libc::kill(pid, libc::SIGKILL) } < 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/process/windows.c",
        original = "moonbitlang_async_terminate_process"
    )]
    #[cfg(windows)]
    pub(crate) fn terminate_process(pid: i32, _signal: i32) -> AsyncHostResult<()> {
        use windows_sys::Win32::System::Console::{GenerateConsoleCtrlEvent, CTRL_BREAK_EVENT};

        let pid = pid as u32;
        // Windows only lets Ctrl-Break reliably target a process group created
        // with CREATE_NEW_PROCESS_GROUP. The upstream C binding returns void and
        // ignores failures, so keep graceful cancellation non-fatal here.
        unsafe {
            GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid);
        }
        Ok(())
    }

    #[ported(
        source = "src/process/windows.c",
        original = "moonbitlang_async_kill_process"
    )]
    #[cfg(windows)]
    pub(crate) fn kill_process(pid: i32) -> AsyncHostResult<()> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

        let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid as u32) };
        if handle.is_null() {
            return Err(last_native_error());
        }
        let result = unsafe { TerminateProcess(handle, 1) };
        let error = if result == 0 {
            Some(last_native_error())
        } else {
            None
        };
        unsafe {
            CloseHandle(handle);
        }
        if let Some(error) = error {
            Err(error)
        } else {
            Ok(())
        }
    }
}

#[cfg(unix)]
pub(crate) fn reap_process(pid: i32) -> AsyncHostResult<()> {
    let mut status = 0;
    let ret = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    if ret < 0 {
        Err(last_native_error())
    } else if ret == pid {
        Ok(())
    } else {
        Err(AsyncHostError::Native(libc::EAGAIN))
    }
}

#[cfg(windows)]
pub(crate) fn process_id_from_handle(
    handle: crate::async_sys::internal::fd_util::stub::RawFd,
) -> AsyncHostResult<i32> {
    let pid = unsafe { windows_sys::Win32::System::Threading::GetProcessId(handle) };
    if pid == 0 {
        Err(last_native_error())
    } else {
        Ok(pid as i32)
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn pidfd_open_is_unsupported(error: AsyncHostError) -> bool {
    matches!(error, AsyncHostError::Native(errno) if errno == libc::ENOSYS || errno == libc::EPERM)
}

#[cfg(windows)]
fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(unsafe { windows_sys::Win32::Foundation::GetLastError() as i32 })
}

#[cfg(unix)]
fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(last_native_errno())
}

#[cfg(target_os = "linux")]
fn last_native_errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

#[cfg(target_os = "macos")]
fn last_native_errno() -> i32 {
    unsafe { *libc::__error() }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use std::os::windows::io::AsRawHandle;
    use std::process::Command;

    use super::*;

    #[test]
    fn running_process_result_is_pending() {
        let mut child = Command::new("cmd.exe")
            .args(["/D", "/C", "ping -n 30 127.0.0.1 >NUL"])
            .spawn()
            .unwrap();
        let result = get_process_result(Some(child.as_raw_handle()), child.id() as i32);

        let _ = child.kill();
        child.wait().unwrap();

        assert_eq!(
            result,
            Err(AsyncHostError::Native(
                windows_sys::Win32::Foundation::ERROR_IO_PENDING as i32
            ))
        );
    }

    #[test]
    fn completed_process_can_return_still_active_value() {
        let mut child = Command::new("cmd.exe")
            .args(["/D", "/C", "exit /B 259"])
            .spawn()
            .unwrap();
        let handle = child.as_raw_handle();
        let pid = child.id() as i32;

        child.wait().unwrap();

        assert_eq!(get_process_result(Some(handle), pid), Ok(259));
    }
}
