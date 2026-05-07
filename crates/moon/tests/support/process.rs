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

use std::{path::Path, process::Child, time::Duration};

pub(crate) fn read_pid_file(pid_file: &Path) -> std::io::Result<u32> {
    let content = std::fs::read_to_string(pid_file)?;
    content
        .trim()
        .parse::<u32>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{e}")))
}

pub(crate) fn wait_for_child_exit(child: &mut Child, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    loop {
        if child
            .try_wait()
            .expect("Failed to poll child process")
            .is_some()
        {
            return true;
        }
        if start.elapsed() > timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub(crate) fn wait_for_pid_exit(pid: u32, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    loop {
        if !pid_is_alive(pid) {
            return true;
        }
        if start.elapsed() > timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(unix)]
pub(crate) fn terminate_child(child: &mut Child) {
    let pid = child.id() as libc::pid_t;
    let rc = unsafe { libc::kill(pid, libc::SIGTERM) };
    if rc != 0 {
        panic!(
            "Failed to send SIGTERM to child process: {}",
            std::io::Error::last_os_error()
        );
    }
}

#[cfg(windows)]
pub(crate) fn terminate_child(child: &mut Child) {
    child.kill().expect("Failed to terminate child process");
}

#[cfg(unix)]
pub(crate) fn terminate_pid(pid: u32) {
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }
}

#[cfg(windows)]
pub(crate) fn terminate_pid(pid: u32) {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FALSE},
        System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess},
    };

    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, FALSE, pid) };
    if !handle.is_null() {
        unsafe {
            TerminateProcess(handle, 1);
            CloseHandle(handle);
        }
    }
}

#[cfg(unix)]
fn pid_is_alive(pid: u32) -> bool {
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        return true;
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => false,
        Some(libc::EPERM) => true,
        _ => true,
    }
}

#[cfg(windows)]
fn pid_is_alive(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, FALSE, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };

    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, FALSE, pid) };
    if handle.is_null() {
        return std::io::Error::last_os_error().raw_os_error()
            != Some(ERROR_INVALID_PARAMETER as i32);
    }
    let wait = unsafe { WaitForSingleObject(handle, 0) };
    unsafe {
        CloseHandle(handle);
    }
    wait != WAIT_OBJECT_0
}
