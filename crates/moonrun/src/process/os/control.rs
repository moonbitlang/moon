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

//! Native process identity, completion and signal mechanisms.

use super::RawFile;
use std::io;
#[cfg(target_os = "linux")]
use std::os::fd::{FromRawFd, OwnedFd};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use std::os::windows::io::{FromRawHandle, OwnedHandle};
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;
use std::process::ExitStatus;

#[cfg(target_os = "linux")]
pub(crate) fn open_pid_handle(pid: i32) -> io::Result<OwnedFd> {
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
    }
}

#[cfg(windows)]
pub(crate) fn open_pid_handle(pid: i32) -> io::Result<OwnedHandle> {
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
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}

pub(crate) fn try_wait(handle: Option<RawFile>, pid: i32) -> io::Result<Option<ExitStatus>> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{
            ERROR_INVALID_HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
        };
        use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
        let _ = pid;
        let handle =
            handle.ok_or_else(|| io::Error::from_raw_os_error(ERROR_INVALID_HANDLE as i32))?;
        // STILL_ACTIVE (259) can also be a real exit code. Inspect the waitable
        // state first, including when callers poll before any blocking wait.
        match unsafe { WaitForSingleObject(handle, 0) } {
            WAIT_OBJECT_0 => {}
            WAIT_TIMEOUT => return Ok(None),
            WAIT_FAILED => return Err(io::Error::last_os_error()),
            _ => return Err(super::invalid_input()),
        }
        let mut code = 0;
        if unsafe { GetExitCodeProcess(handle, &mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Some(ExitStatus::from_raw(code)))
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
                return Err(io::Error::last_os_error());
            }
            return Ok((unsafe { info.si_pid() } != 0).then(|| super::siginfo_status(&info)));
        }
        let mut status = 0;
        let ret = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((ret != 0).then(|| ExitStatus::from_raw(status)))
    }
}

pub(crate) fn signal(pid: i32, signal: i32) -> io::Result<()> {
    #[cfg(unix)]
    let failed = unsafe { libc::kill(pid, signal) } < 0;
    #[cfg(windows)]
    let failed = {
        use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, GenerateConsoleCtrlEvent};
        let _ = signal;
        (unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid as u32) }) == 0
    };
    if failed {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub(crate) fn kill(pid: i32) -> io::Result<()> {
    #[cfg(unix)]
    {
        signal(pid, libc::SIGKILL)
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_TERMINATE, TerminateProcess,
        };
        let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid as u32) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        if unsafe { TerminateProcess(handle.as_raw_handle(), 1) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(unix)]
pub(crate) fn reap(pid: i32) -> io::Result<()> {
    let mut status = 0;
    let ret = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else if ret == pid {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(libc::EAGAIN))
    }
}

#[cfg(windows)]
pub(crate) fn process_id(handle: RawFile) -> io::Result<i32> {
    let pid = unsafe { windows_sys::Win32::System::Threading::GetProcessId(handle) };
    if pid == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(pid as i32)
    }
}
