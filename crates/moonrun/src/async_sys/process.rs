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
    pub(crate) fn terminate_process(pid: i32, signal: i32) -> AsyncHostResult<()> {
        use windows_sys::Win32::System::Console::GenerateConsoleCtrlEvent;

        if unsafe { GenerateConsoleCtrlEvent(signal as u32, pid as u32) } == 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
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
        unsafe {
            CloseHandle(handle);
        }
        if result == 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}
