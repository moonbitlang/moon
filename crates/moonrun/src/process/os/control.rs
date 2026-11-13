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

use std::io;
#[cfg(target_os = "linux")]
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;
use std::process::ExitStatus;

#[cfg(target_os = "linux")]
pub(crate) fn open_pidfd(pid: libc::pid_t) -> io::Result<OwnedFd> {
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(fd as RawFd) })
    }
}

#[cfg(windows)]
pub(crate) fn open_process_for_wait(pid: u32) -> io::Result<OwnedHandle> {
    use windows_sys::Win32::Storage::FileSystem::SYNCHRONIZE;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    let handle = unsafe { OpenProcess(SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}

#[cfg(windows)]
pub(crate) fn try_wait_handle(handle: RawHandle) -> io::Result<Option<ExitStatus>> {
    use windows_sys::Win32::Foundation::{WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
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

#[cfg(target_os = "linux")]
pub(crate) fn try_wait_pidfd(pidfd: RawFd) -> io::Result<Option<ExitStatus>> {
    let mut info = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
    if unsafe {
        libc::waitid(
            libc::P_PIDFD,
            pidfd as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok((unsafe { info.si_pid() } != 0).then(|| super::siginfo_status(&info)))
}

#[cfg(unix)]
pub(crate) fn try_wait_pid(pid: libc::pid_t) -> io::Result<Option<ExitStatus>> {
    let mut status = 0;
    let ret = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((ret != 0).then(|| ExitStatus::from_raw(status)))
}

#[cfg(unix)]
pub(crate) fn send_signal(pid: libc::pid_t, signal: libc::c_int) -> io::Result<()> {
    if unsafe { libc::kill(pid, signal) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub(crate) fn send_console_event(group_id: u32, event: u32) -> io::Result<()> {
    if unsafe { windows_sys::Win32::System::Console::GenerateConsoleCtrlEvent(event, group_id) }
        == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub(crate) fn terminate_process(pid: u32, exit_code: u32) -> io::Result<()> {
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE};
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
    terminate_handle(handle.as_raw_handle(), exit_code)
}

#[cfg(windows)]
pub(crate) fn terminate_handle(handle: RawHandle, exit_code: u32) -> io::Result<()> {
    if unsafe { windows_sys::Win32::System::Threading::TerminateProcess(handle, exit_code) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
pub(crate) fn reap(pid: libc::pid_t) -> io::Result<()> {
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
pub(crate) fn process_id(handle: RawHandle) -> io::Result<u32> {
    let pid = unsafe { windows_sys::Win32::System::Threading::GetProcessId(handle) };
    if pid == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(pid)
    }
}
