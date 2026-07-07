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

//! Process jobs ported from `moonbitlang/async/src/internal/event_loop/thread_pool.c`.

use std::ffi::OsString;
#[cfg(unix)]
use std::ffi::{CStr, CString};

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::fd_util;
use crate::async_sys::ported_fns;

#[cfg(windows)]
use super::Resource;
use super::{OpenJobResource, ResourceRef};

type RawFile = fd_util::stub::RawFd;

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "spawn_job_worker"
    )]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_spawn_job(
        path: OsString,
        args: Vec<OsString>,
        env: Vec<(OsString, OsString)>,
        inherit_env: bool,
        stdio: [Option<ResourceRef>; 3],
        cwd: Option<OsString>,
        result: &mut Option<OpenJobResource>,
    ) -> AsyncHostResult<i64> {
        spawn_process(
            path,
            args,
            env,
            inherit_env,
            stdio,
            cwd,
            result,
        )
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "wait_for_process_job_worker"
    )]
    pub(super) fn run_wait_for_process_job(
        handle: Option<ResourceRef>,
        pid: i32,
    ) -> AsyncHostResult<i64> {
        wait_for_process(handle, pid)
    }
}

#[cfg(unix)]
#[allow(clippy::too_many_arguments)]
fn spawn_process(
    path: OsString,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    inherit_env: bool,
    stdio: [Option<ResourceRef>; 3],
    cwd: Option<OsString>,
    _result: &mut Option<OpenJobResource>,
) -> AsyncHostResult<i64> {
    let mut argv_storage = Vec::with_capacity(args.len() + 1);
    argv_storage.push(unix_cstring(path)?);
    for arg in args {
        argv_storage.push(unix_cstring(arg)?);
    }
    let path = argv_storage[0].as_c_str();
    let mut argv = argv_storage
        .iter()
        .map(|arg| arg.as_ptr().cast_mut())
        .collect::<Vec<_>>();
    argv.push(std::ptr::null_mut());

    let env_storage = unix_env(env, inherit_env)?;
    let mut envp = env_storage
        .iter()
        .map(|entry| entry.as_ptr().cast_mut())
        .collect::<Vec<_>>();
    envp.push(std::ptr::null_mut());

    let cwd = cwd.map(unix_cstring).transpose()?;
    let stdio_fds = duplicate_stdio_fds(&stdio)?;

    let mut attr = unsafe { std::mem::zeroed::<libc::posix_spawnattr_t>() };
    let mut file_actions = unsafe { std::mem::zeroed::<libc::posix_spawn_file_actions_t>() };
    let mut attr_initialized = false;
    let mut file_actions_initialized = false;

    let spawn_result = (|| -> Result<libc::pid_t, i32> {
        check_spawn_errno(unsafe { libc::posix_spawnattr_init(&mut attr) })?;
        attr_initialized = true;

        let flags = (libc::POSIX_SPAWN_SETSIGMASK | libc::POSIX_SPAWN_SETSIGDEF) as libc::c_short;
        check_spawn_errno(unsafe { libc::posix_spawnattr_setflags(&mut attr, flags) })?;

        let mut sigmask = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        check_spawn_errno(unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &mut sigmask)
        })?;
        check_spawn_errno(unsafe { libc::posix_spawnattr_setsigmask(&mut attr, &sigmask) })?;

        let mut all_signals = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        if unsafe { libc::sigfillset(&mut all_signals) } != 0 {
            return Err(last_native_errno());
        }
        check_spawn_errno(unsafe { libc::posix_spawnattr_setsigdefault(&mut attr, &all_signals) })?;

        check_spawn_errno(unsafe { libc::posix_spawn_file_actions_init(&mut file_actions) })?;
        file_actions_initialized = true;
        for (target, fd) in stdio_fds.iter().enumerate() {
            if let Some(fd) = fd {
                check_spawn_errno(unsafe {
                    libc::posix_spawn_file_actions_adddup2(
                        &mut file_actions,
                        *fd,
                        target as libc::c_int,
                    )
                })?;
            }
        }
        if let Some(cwd) = cwd.as_ref() {
            check_spawn_errno(add_chdir_file_action(&mut file_actions, cwd.as_c_str()))?;
        }

        let mut pid = 0;
        let ret = if path.to_bytes().contains(&b'/') {
            unsafe {
                libc::posix_spawn(
                    &mut pid,
                    path.as_ptr(),
                    &file_actions,
                    &attr,
                    argv.as_ptr(),
                    envp.as_ptr(),
                )
            }
        } else {
            unsafe {
                libc::posix_spawnp(
                    &mut pid,
                    path.as_ptr(),
                    &file_actions,
                    &attr,
                    argv.as_ptr(),
                    envp.as_ptr(),
                )
            }
        };
        check_spawn_errno(ret)?;
        Ok(pid)
    })();

    if attr_initialized {
        unsafe {
            libc::posix_spawnattr_destroy(&mut attr);
        }
    }
    if file_actions_initialized {
        unsafe {
            libc::posix_spawn_file_actions_destroy(&mut file_actions);
        }
    }
    close_stdio_fds(&stdio_fds);

    spawn_result.map(i64::from).map_err(AsyncHostError::Native)
}

#[cfg(unix)]
fn unix_cstring(value: OsString) -> AsyncHostResult<CString> {
    use std::os::unix::ffi::OsStrExt;

    CString::new(value.as_os_str().as_bytes()).map_err(|_| AsyncHostError::Inval)
}

#[cfg(unix)]
fn unix_env(env: Vec<(OsString, OsString)>, inherit_env: bool) -> AsyncHostResult<Vec<CString>> {
    let mut entries = Vec::new();
    if inherit_env {
        for (key, value) in std::env::vars_os() {
            entries.push(unix_env_entry(key, value)?);
        }
    }
    for (key, value) in env {
        entries.push(unix_env_entry(key, value)?);
    }
    Ok(entries)
}

#[cfg(unix)]
fn unix_env_entry(key: OsString, value: OsString) -> AsyncHostResult<CString> {
    use std::os::unix::ffi::OsStrExt;

    let mut entry = key.as_os_str().as_bytes().to_vec();
    entry.push(b'=');
    entry.extend_from_slice(value.as_os_str().as_bytes());
    CString::new(entry).map_err(|_| AsyncHostError::Inval)
}

#[cfg(unix)]
fn duplicate_stdio_fds(stdio: &[Option<ResourceRef>; 3]) -> AsyncHostResult<[Option<RawFile>; 3]> {
    let mut fds = [None, None, None];
    for (index, resource) in stdio.iter().enumerate() {
        let Some(resource) = resource else {
            continue;
        };
        let fd = unsafe { libc::fcntl(resource.raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
        if fd < 0 {
            let error = last_native_error();
            close_stdio_fds(&fds);
            return Err(error);
        }
        fds[index] = Some(fd);
    }
    Ok(fds)
}

#[cfg(unix)]
fn close_stdio_fds(fds: &[Option<RawFile>; 3]) {
    for fd in fds.iter().flatten() {
        unsafe {
            libc::close(*fd);
        }
    }
}

#[cfg(unix)]
fn check_spawn_errno(ret: libc::c_int) -> Result<(), i32> {
    if ret == 0 { Ok(()) } else { Err(ret) }
}

#[cfg(target_os = "linux")]
fn add_chdir_file_action(
    file_actions: *mut libc::posix_spawn_file_actions_t,
    cwd: &CStr,
) -> libc::c_int {
    unsafe { libc::posix_spawn_file_actions_addchdir_np(file_actions, cwd.as_ptr()) }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn posix_spawn_file_actions_addchdir_np(
        file_actions: *mut libc::posix_spawn_file_actions_t,
        cwd: *const libc::c_char,
    ) -> libc::c_int;
}

#[cfg(target_os = "macos")]
fn add_chdir_file_action(
    file_actions: *mut libc::posix_spawn_file_actions_t,
    cwd: &CStr,
) -> libc::c_int {
    unsafe { posix_spawn_file_actions_addchdir_np(file_actions, cwd.as_ptr()) }
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn add_chdir_file_action(
    _file_actions: *mut libc::posix_spawn_file_actions_t,
    _cwd: &CStr,
) -> libc::c_int {
    libc::ENOSYS
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn spawn_process(
    path: OsString,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    inherit_env: bool,
    stdio: [Option<ResourceRef>; 3],
    cwd: Option<OsString>,
    result: &mut Option<OpenJobResource>,
) -> AsyncHostResult<i64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    use windows_sys::Win32::System::Threading::{
        CREATE_NEW_PROCESS_GROUP, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
        DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
        InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION,
        STARTF_USESTDHANDLES, STARTUPINFOEXW, UpdateProcThreadAttribute,
    };

    let mut command_line = windows_command_line(&path, &args);
    let env_block = windows_env_block(env, inherit_env);
    let cwd = cwd.map(|cwd| {
        let mut cwd = cwd.encode_wide().collect::<Vec<_>>();
        cwd.push(0);
        cwd
    });

    let std_handles = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE];
    let mut inherited_handles = Vec::with_capacity(std_handles.len());
    for (resource, std_handle) in stdio.iter().zip(std_handles) {
        let raw_result = if let Some(resource) = resource {
            Ok(resource.raw_fd())
        } else {
            let raw = unsafe { GetStdHandle(std_handle) };
            if raw.is_null() || raw == INVALID_HANDLE_VALUE {
                Err(last_native_error())
            } else {
                Ok(raw)
            }
        };
        let raw = match raw_result {
            Ok(raw) => raw,
            Err(error) => {
                close_handles(&inherited_handles);
                return Err(error);
            }
        };
        match duplicate_inheritable_handle(raw) {
            Ok(handle) => inherited_handles.push(handle),
            Err(error) => {
                close_handles(&inherited_handles);
                return Err(error);
            }
        }
    }

    let mut startup_info = unsafe { std::mem::zeroed::<STARTUPINFOEXW>() };
    startup_info.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup_info.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup_info.StartupInfo.hStdInput = inherited_handles[0];
    startup_info.StartupInfo.hStdOutput = inherited_handles[1];
    startup_info.StartupInfo.hStdError = inherited_handles[2];

    let mut attrs_size = 0;
    unsafe {
        InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut attrs_size);
    }
    let mut attrs = vec![0u8; attrs_size];
    startup_info.lpAttributeList = attrs.as_mut_ptr().cast();
    if unsafe {
        InitializeProcThreadAttributeList(startup_info.lpAttributeList, 1, 0, &mut attrs_size)
    } == 0
    {
        let error = last_native_error();
        close_handles(&inherited_handles);
        return Err(error);
    }

    if unsafe {
        UpdateProcThreadAttribute(
            startup_info.lpAttributeList,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            inherited_handles.as_ptr().cast(),
            inherited_handles.len() * std::mem::size_of::<HANDLE>(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        let error = last_native_error();
        unsafe {
            DeleteProcThreadAttributeList(startup_info.lpAttributeList);
        }
        close_handles(&inherited_handles);
        return Err(error);
    }

    let mut process_info = unsafe { std::mem::zeroed::<PROCESS_INFORMATION>() };
    let created = unsafe {
        CreateProcessW(
            std::ptr::null(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_NEW_PROCESS_GROUP | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            env_block.as_ptr().cast(),
            cwd.as_ref().map_or(std::ptr::null(), |cwd| cwd.as_ptr()),
            (&raw mut startup_info.StartupInfo).cast(),
            &mut process_info,
        )
    };

    unsafe {
        DeleteProcThreadAttributeList(startup_info.lpAttributeList);
    }
    close_handles(&inherited_handles);

    if created == 0 {
        return Err(last_native_error());
    }

    unsafe {
        CloseHandle(process_info.hThread);
    }
    *result = Some(OpenJobResource::Unpublished(Resource::new(
        process_info.hProcess,
    )));
    Ok(i64::from(process_info.dwProcessId))
}

#[cfg(windows)]
fn windows_command_line(path: &OsString, args: &[OsString]) -> Vec<u16> {
    let mut command_line = Vec::new();
    push_windows_arg(&mut command_line, path);
    for arg in args {
        command_line.push(' ' as u16);
        push_windows_arg(&mut command_line, arg);
    }
    command_line.push(0);
    command_line
}

#[cfg(windows)]
fn push_windows_arg(command_line: &mut Vec<u16>, arg: &OsString) {
    use std::os::windows::ffi::OsStrExt;

    let arg = arg.encode_wide().collect::<Vec<_>>();
    let needs_quote = arg.is_empty()
        || arg
            .iter()
            .any(|unit| *unit == b' ' as u16 || *unit == b'\t' as u16 || *unit == b'"' as u16);
    if !needs_quote {
        command_line.extend_from_slice(&arg);
        return;
    }

    command_line.push(b'"' as u16);
    let mut backslashes = 0;
    for unit in arg {
        match unit {
            unit if unit == b'\\' as u16 => backslashes += 1,
            unit if unit == b'"' as u16 => {
                command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2 + 1));
                command_line.push(unit);
                backslashes = 0;
            }
            unit => {
                command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
                command_line.push(unit);
                backslashes = 0;
            }
        }
    }
    command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
    command_line.push(b'"' as u16);
}

#[cfg(windows)]
fn windows_env_block(env: Vec<(OsString, OsString)>, inherit_env: bool) -> Vec<u16> {
    use std::collections::BTreeMap;
    use std::os::windows::ffi::OsStrExt;

    let mut entries = BTreeMap::new();
    if inherit_env {
        for (key, value) in std::env::vars_os() {
            entries.insert(windows_env_key(&key), (key, value));
        }
    }
    for (key, value) in env {
        entries.insert(windows_env_key(&key), (key, value));
    }

    let mut block = Vec::new();
    for (_, (key, value)) in entries {
        block.extend(key.encode_wide());
        block.push(b'=' as u16);
        block.extend(value.encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

#[cfg(windows)]
fn windows_env_key(key: &OsString) -> String {
    key.to_string_lossy().to_ascii_uppercase()
}

#[cfg(unix)]
fn wait_for_process(_handle: Option<ResourceRef>, pid: i32) -> AsyncHostResult<i64> {
    let mut status = 0;
    let ret = unsafe { libc::waitpid(pid, &mut status, 0) };
    if ret < 0 {
        return Err(last_native_error());
    }
    if ret != pid {
        return Err(AsyncHostError::Inval);
    }
    if libc::WIFEXITED(status) {
        Ok(i64::from(libc::WEXITSTATUS(status)))
    } else if libc::WIFSIGNALED(status) {
        Ok(i64::from(128 + libc::WTERMSIG(status)))
    } else {
        Ok(1)
    }
}

#[cfg(windows)]
fn wait_for_process(handle: Option<ResourceRef>, pid: i32) -> AsyncHostResult<i64> {
    if let Some(handle) = handle {
        return wait_for_process_handle(handle.raw_fd());
    }

    use windows_sys::Win32::Foundation::CloseHandle;
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
        return Err(last_native_error());
    }
    let result = wait_for_process_handle(handle);
    unsafe {
        CloseHandle(handle);
    }
    result
}

#[cfg(windows)]
fn wait_for_process_handle(handle: RawFile) -> AsyncHostResult<i64> {
    use windows_sys::Win32::Foundation::{WAIT_FAILED, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};

    let wait = unsafe { WaitForSingleObject(handle, u32::MAX) };
    if wait == WAIT_FAILED || wait != WAIT_OBJECT_0 {
        return Err(last_native_error());
    }
    let mut code = 0;
    if unsafe { GetExitCodeProcess(handle, &mut code) } == 0 {
        return Err(last_native_error());
    }
    Ok(i64::from(code))
}

#[cfg(windows)]
fn duplicate_inheritable_handle(raw: RawFile) -> AsyncHostResult<RawFile> {
    use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let process = unsafe { GetCurrentProcess() };
    let mut duplicate: HANDLE = std::ptr::null_mut();
    if unsafe {
        DuplicateHandle(
            process,
            raw,
            process,
            &mut duplicate,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
    {
        Err(last_native_error())
    } else {
        Ok(duplicate)
    }
}

#[cfg(windows)]
fn close_handles(handles: &[RawFile]) {
    use windows_sys::Win32::Foundation::CloseHandle;

    for handle in handles {
        unsafe {
            CloseHandle(*handle);
        }
    }
}

fn io_error(error: std::io::Error) -> AsyncHostError {
    AsyncHostError::Native(
        error
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}

fn last_native_error() -> AsyncHostError {
    io_error(std::io::Error::last_os_error())
}

#[cfg(unix)]
fn last_native_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_else(|| AsyncHostError::Inval.errno())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn wait_for_unknown_process_reports_native_error() {
        let err = run_wait_for_process_job(None, -1).unwrap_err();
        assert!(matches!(err, AsyncHostError::Native(_)));
    }
}
