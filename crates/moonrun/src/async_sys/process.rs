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

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::ported_fns;

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/process.c",
        original = "moonbitlang_async_open_pid_handle"
    )]
    #[cfg(target_os = "linux")]
    pub(crate) fn open_pid_handle(
        pid: i32,
    ) -> AsyncHostResult<crate::filesystem::RawFile> {
        use std::os::fd::IntoRawFd;
        Ok(crate::process::os::open_pidfd(pid)?.into_raw_fd())
    }

    #[ported(
        source = "src/internal/event_loop/process.c",
        original = "moonbitlang_async_open_pid_handle"
    )]
    #[cfg(windows)]
    pub(crate) fn open_pid_handle(
        pid: i32,
    ) -> AsyncHostResult<crate::filesystem::RawFile> {
        use std::os::windows::io::IntoRawHandle;
        Ok(crate::process::os::open_process_for_wait(pid as u32)?.into_raw_handle())
    }

    #[ported(
        source = "src/internal/event_loop/process.c",
        original = "moonbitlang_async_get_process_result"
    )]
    pub(crate) fn get_process_result(
        handle: Option<crate::filesystem::RawFile>,
        pid: i32,
    ) -> AsyncHostResult<i32> {
        #[cfg(windows)]
        let status = {
            let _ = pid;
            crate::process::os::try_wait_handle(handle.ok_or(AsyncHostError::Badf)?)?
        };
        #[cfg(target_os = "linux")]
        let status = match handle {
            Some(pidfd) => crate::process::os::try_wait_pidfd(pidfd)?,
            None => crate::process::os::try_wait_pid(pid)?,
        };
        #[cfg(all(unix, not(target_os = "linux")))]
        let status = {
            let _ = handle;
            crate::process::os::try_wait_pid(pid)?
        };
        match status {
            Some(status) => Ok(crate::process::native::exit_code(status) as i32),
            None => {
                #[cfg(unix)]
                let pending = libc::EAGAIN;
                #[cfg(windows)]
                let pending = windows_sys::Win32::Foundation::ERROR_IO_PENDING as i32;
                Err(AsyncHostError::Native(pending))
            }
        }
    }

    #[ported(
        source = "src/process/unix.c",
        original = "moonbitlang_async_terminate_process"
    )]
    #[cfg(unix)]
    pub(crate) fn terminate_process(pid: i32, signal: i32) -> AsyncHostResult<()> {
        crate::process::os::send_signal(pid, signal).map_err(Into::into)
    }

    #[ported(
        source = "src/process/unix.c",
        original = "moonbitlang_async_kill_process"
    )]
    #[cfg(unix)]
    pub(crate) fn kill_process(pid: i32) -> AsyncHostResult<()> {
        crate::process::os::send_signal(pid, libc::SIGKILL).map_err(Into::into)
    }

    #[ported(
        source = "src/process/windows.c",
        original = "moonbitlang_async_terminate_process"
    )]
    #[cfg(windows)]
    pub(crate) fn terminate_process(pid: i32, _signal: i32) -> AsyncHostResult<()> {
        // The native async ABI ignores failures from graceful cancellation.
        let _ = crate::process::os::send_console_event(
            pid as u32,
            windows_sys::Win32::System::Console::CTRL_BREAK_EVENT,
        );
        Ok(())
    }

    #[ported(
        source = "src/process/windows.c",
        original = "moonbitlang_async_kill_process"
    )]
    #[cfg(windows)]
    pub(crate) fn kill_process(pid: i32) -> AsyncHostResult<()> {
        crate::process::os::terminate_process(pid as u32, 1).map_err(Into::into)
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn pidfd_open_is_unsupported(error: AsyncHostError) -> bool {
    matches!(error, AsyncHostError::Native(errno) if errno == libc::ENOSYS || errno == libc::EPERM)
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn unix_wait_status_distinguishes_exit_from_signal() {
        assert_eq!(
            crate::process::native::exit_code(std::process::ExitStatus::from_raw(7 << 8)),
            7
        );
        assert_eq!(
            crate::process::native::exit_code(std::process::ExitStatus::from_raw(libc::SIGTERM)),
            -i64::from(libc::SIGTERM)
        );
    }
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
