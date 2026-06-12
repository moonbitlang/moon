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

use std::sync::{Arc, Mutex};

use crate::async_host::{AsyncHostError, AsyncHostResult};

use super::jobs;
use super::types::{HostFileTable, HostHandle, Job};

#[derive(Debug, Clone)]
pub(crate) struct HostProcess {
    process: Arc<Mutex<Option<NativeProcess>>>,
}

impl PartialEq for HostProcess {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.process, &other.process)
    }
}

impl Eq for HostProcess {}

impl HostProcess {
    fn new(process: NativeProcess) -> Self {
        Self {
            process: Arc::new(Mutex::new(Some(process))),
        }
    }

    fn wait(&self) -> AsyncHostResult<i32> {
        let mut process = self.process.lock().unwrap();
        let Some(process) = process.take() else {
            return Err(AsyncHostError::Badf);
        };
        process.wait()
    }
}

#[cfg(unix)]
#[derive(Debug, PartialEq, Eq)]
struct NativeProcess {
    pid: libc::pid_t,
}

#[cfg(unix)]
impl NativeProcess {
    fn wait(self) -> AsyncHostResult<i32> {
        let mut status = 0;
        let ret = unsafe { libc::waitpid(self.pid, &mut status, 0) };
        if ret == self.pid {
            Ok(libc::WEXITSTATUS(status))
        } else {
            Err(last_native_error())
        }
    }
}

#[cfg(windows)]
#[derive(Debug, PartialEq, Eq)]
struct NativeProcess {
    handle: isize,
}

#[cfg(windows)]
impl NativeProcess {
    fn handle(&self) -> windows_sys::Win32::Foundation::HANDLE {
        self.handle as windows_sys::Win32::Foundation::HANDLE
    }

    fn wait(self) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Foundation::WAIT_FAILED;
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, INFINITE, WaitForSingleObject,
        };

        if unsafe { WaitForSingleObject(self.handle(), INFINITE) } == WAIT_FAILED {
            return Err(last_native_error());
        }
        let mut exit_code = 0;
        if unsafe { GetExitCodeProcess(self.handle(), &mut exit_code) } == 0 {
            return Err(last_native_error());
        }
        Ok(exit_code as i32)
    }
}

#[cfg(windows)]
impl Drop for NativeProcess {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle());
        }
    }
}

pub(crate) trait HostProcessTable {
    fn insert_process(&mut self, process: HostProcess) -> AsyncHostResult<HostHandle>;

    fn take_process(&mut self, handle: HostHandle) -> AsyncHostResult<HostProcess>;
}

pub(crate) fn spawn_process(
    files: &mut impl HostFileTable,
    processes: &mut impl HostProcessTable,
    command: String,
    args: Vec<String>,
    stdin: HostHandle,
    stdout: HostHandle,
    stderr: HostHandle,
) -> AsyncHostResult<HostHandle> {
    spawn_native_process(files, processes, command, args, stdin, stdout, stderr)
}

#[cfg(unix)]
fn spawn_native_process(
    files: &mut impl HostFileTable,
    processes: &mut impl HostProcessTable,
    command: String,
    args: Vec<String>,
    stdin: HostHandle,
    stdout: HostHandle,
    stderr: HostHandle,
) -> AsyncHostResult<HostHandle> {
    use std::ffi::CString;

    let command_c = CString::new(command.clone()).map_err(|_| AsyncHostError::Inval)?;
    let mut argv_c = Vec::with_capacity(args.len() + 1);
    argv_c.push(CString::new(command.as_str()).map_err(|_| AsyncHostError::Inval)?);
    for arg in args {
        argv_c.push(CString::new(arg).map_err(|_| AsyncHostError::Inval)?);
    }
    let mut argv = argv_c
        .iter()
        .map(|arg| arg.as_ptr() as *mut libc::c_char)
        .collect::<Vec<_>>();
    argv.push(std::ptr::null_mut());

    let env_c = current_environment();
    let mut envp = env_c
        .iter()
        .map(|env| env.as_ptr() as *mut libc::c_char)
        .collect::<Vec<_>>();
    envp.push(std::ptr::null_mut());

    let stdio = [
        raw_fd_for_stdio(files, stdin)?,
        raw_fd_for_stdio(files, stdout)?,
        raw_fd_for_stdio(files, stderr)?,
    ];

    let mut file_actions = std::mem::MaybeUninit::<libc::posix_spawn_file_actions_t>::uninit();
    let ret = unsafe { libc::posix_spawn_file_actions_init(file_actions.as_mut_ptr()) };
    if ret != 0 {
        return Err(AsyncHostError::Native(ret));
    }
    let mut file_actions = unsafe { file_actions.assume_init() };

    let result = (|| {
        for (target, fd) in stdio.into_iter().enumerate() {
            if let Some(fd) = fd {
                let ret = unsafe {
                    libc::posix_spawn_file_actions_adddup2(
                        &mut file_actions,
                        fd,
                        target as libc::c_int,
                    )
                };
                if ret != 0 {
                    return Err(AsyncHostError::Native(ret));
                }
            }
        }

        let mut pid = 0;
        // The native C stub also restores the thread-pool signal mask here.
        // The wasm host does not install that signal mask yet, so this port
        // keeps the same spawn function and stdio file actions.
        let ret = if command_c.as_bytes().contains(&b'/') {
            unsafe {
                libc::posix_spawn(
                    &mut pid,
                    command_c.as_ptr(),
                    &file_actions,
                    std::ptr::null(),
                    argv.as_mut_ptr(),
                    envp.as_mut_ptr(),
                )
            }
        } else {
            unsafe {
                libc::posix_spawnp(
                    &mut pid,
                    command_c.as_ptr(),
                    &file_actions,
                    std::ptr::null(),
                    argv.as_mut_ptr(),
                    envp.as_mut_ptr(),
                )
            }
        };
        if ret != 0 {
            return Err(AsyncHostError::Native(ret));
        }
        processes.insert_process(HostProcess::new(NativeProcess { pid }))
    })();

    unsafe {
        libc::posix_spawn_file_actions_destroy(&mut file_actions);
    }
    result
}

#[cfg(unix)]
fn raw_fd_for_stdio(
    files: &mut impl HostFileTable,
    handle: HostHandle,
) -> AsyncHostResult<Option<libc::c_int>> {
    use std::os::fd::AsRawFd;

    if handle < 0 {
        return Ok(None);
    }
    files.with_file_mut(handle, |file| Ok(Some(file.as_raw_fd())))
}

#[cfg(all(unix, target_os = "linux"))]
unsafe fn current_environ() -> *mut *mut libc::c_char {
    unsafe extern "C" {
        static mut environ: *mut *mut libc::c_char;
    }

    unsafe { environ }
}

#[cfg(all(unix, target_os = "macos"))]
unsafe fn current_environ() -> *mut *mut libc::c_char {
    unsafe { *libc::_NSGetEnviron() }
}

#[cfg(unix)]
fn current_environment() -> Vec<std::ffi::CString> {
    let mut env = Vec::new();
    let mut cursor = unsafe { current_environ() };
    if cursor.is_null() {
        return env;
    }
    while unsafe { !(*cursor).is_null() } {
        env.push(unsafe { std::ffi::CStr::from_ptr(*cursor).to_owned() });
        cursor = unsafe { cursor.add(1) };
    }
    env
}

#[cfg(windows)]
fn spawn_native_process(
    files: &mut impl HostFileTable,
    processes: &mut impl HostProcessTable,
    command: String,
    args: Vec<String>,
    stdin: HostHandle,
    stdout: HostHandle,
    stderr: HostHandle,
) -> AsyncHostResult<HostHandle> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    use windows_sys::Win32::System::Threading::{
        CREATE_NEW_PROCESS_GROUP, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, PROCESS_INFORMATION,
        STARTF_USESTDHANDLES, STARTUPINFOW,
    };

    let stdio = [
        raw_handle_for_stdio(files, stdin, STD_INPUT_HANDLE)?,
        raw_handle_for_stdio(files, stdout, STD_OUTPUT_HANDLE)?,
        raw_handle_for_stdio(files, stderr, STD_ERROR_HANDLE)?,
    ];
    for handle in stdio {
        if handle == INVALID_HANDLE_VALUE as isize {
            return Err(last_native_error());
        }
        if unsafe {
            windows_sys::Win32::Foundation::SetHandleInformation(
                handle as _,
                windows_sys::Win32::Foundation::HANDLE_FLAG_INHERIT,
                windows_sys::Win32::Foundation::HANDLE_FLAG_INHERIT,
            )
        } == 0
        {
            return Err(last_native_error());
        }
    }

    let mut command_line = windows_command_line(command, args)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let startup_info = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        dwFlags: STARTF_USESTDHANDLES,
        hStdInput: stdio[0] as _,
        hStdOutput: stdio[1] as _,
        hStdError: stdio[2] as _,
        ..unsafe { std::mem::zeroed() }
    };
    let mut process_info = unsafe { std::mem::zeroed::<PROCESS_INFORMATION>() };
    let ok = unsafe {
        CreateProcessW(
            std::ptr::null(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_NEW_PROCESS_GROUP | CREATE_UNICODE_ENVIRONMENT,
            std::ptr::null(),
            std::ptr::null(),
            &startup_info,
            &mut process_info,
        )
    };
    if ok == 0 {
        return Err(last_native_error());
    }
    unsafe {
        windows_sys::Win32::Foundation::CloseHandle(process_info.hThread);
    }
    processes.insert_process(HostProcess::new(NativeProcess {
        handle: process_info.hProcess as isize,
    }))
}

#[cfg(windows)]
fn raw_handle_for_stdio(
    files: &mut impl HostFileTable,
    handle: HostHandle,
    default: u32,
) -> AsyncHostResult<isize> {
    use std::os::windows::io::AsRawHandle;

    if handle < 0 {
        return Ok(unsafe { windows_sys::Win32::System::Console::GetStdHandle(default) } as isize);
    }
    files.with_file_mut(handle, |file| Ok(file.as_raw_handle() as isize))
}

#[cfg(windows)]
fn windows_command_line(command: String, args: Vec<String>) -> String {
    let command = if command.ends_with(".exe") {
        command
    } else {
        format!("{command}.exe")
    };
    let mut line = String::new();
    push_windows_arg(&mut line, &command);
    for arg in args {
        line.push(' ');
        push_windows_arg(&mut line, &arg);
    }
    line
}

#[cfg(windows)]
fn push_windows_arg(out: &mut String, arg: &str) {
    let need_quote = arg.chars().any(|ch| matches!(ch, ' ' | '\t' | '"'));
    if !need_quote {
        out.push_str(arg);
        return;
    }

    out.push('"');
    let mut backslashes = 0;
    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                for _ in 0..(backslashes * 2 + 1) {
                    out.push('\\');
                }
                out.push('"');
                backslashes = 0;
            }
            _ => {
                for _ in 0..backslashes {
                    out.push('\\');
                }
                out.push(ch);
                backslashes = 0;
            }
        }
    }
    for _ in 0..(backslashes * 2) {
        out.push('\\');
    }
    out.push('"');
}

pub(crate) fn make_wait_for_process_job_from_handle(
    processes: &mut impl HostProcessTable,
    process: HostHandle,
) -> AsyncHostResult<Job> {
    Ok(jobs::make_wait_for_process_job(
        processes.take_process(process)?,
    ))
}

#[allow(dead_code)]
pub(super) fn run_wait_for_process_job(process: &HostProcess) -> AsyncHostResult<i64> {
    process.wait().map(i64::from)
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}
