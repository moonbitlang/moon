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

use std::ffi::{OsStr, OsString};

#[cfg(unix)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LegacyProcessEnvEntry {
    // Entries copied from the host environment are already materialized as
    // `key=value`. Guest-provided entries keep their boundary until the
    // legacy buffer is converted into the current builder.
    Materialized(OsString),
    Added { key: OsString, value: OsString },
}

#[cfg(unix)]
impl LegacyProcessEnvEntry {
    fn into_materialized(self) -> Option<OsString> {
        match self {
            Self::Materialized(entry) => Some(entry),
            Self::Added { key, value } => unix_process_env_entry(key, value),
        }
    }
}

// Keep the inherited snapshot separate until spawn: native writes user entries
// first, then copies only inherited entries whose keys were not overridden.
pub(crate) struct ProcessEnvBuilder {
    extra: Vec<OsString>,
    inherited: Vec<OsString>,
}

impl ProcessEnvBuilder {
    pub(crate) fn new(inherited: Vec<OsString>) -> Self {
        Self {
            extra: Vec::new(),
            inherited,
        }
    }

    #[cfg(unix)]
    pub(crate) fn from_legacy_env(
        mut entries: Vec<LegacyProcessEnvEntry>,
        inherited_entry_count: usize,
    ) -> Self {
        let extra = entries.split_off(inherited_entry_count);
        Self {
            extra: extra
                .into_iter()
                .filter_map(LegacyProcessEnvEntry::into_materialized)
                .collect(),
            inherited: entries
                .into_iter()
                .filter_map(LegacyProcessEnvEntry::into_materialized)
                .collect(),
        }
    }
}

#[cfg(unix)]
pub(crate) fn overwrite_process_env_var(env: &mut Vec<OsString>, key: &OsStr, value: &OsStr) {
    let mut builder = ProcessEnvBuilder::new(std::mem::take(env));
    process_env_builder_add_entry(&mut builder, key.to_owned(), value.to_owned());
    *env = finish_process_env_builder(builder);
}

#[cfg(windows)]
pub(crate) fn overwrite_process_env_var(env: &mut Vec<u16>, key: &OsStr, value: &OsStr) {
    use std::os::windows::ffi::OsStringExt;

    let inherited = env
        .split(|unit| *unit == 0)
        .take_while(|entry| !entry.is_empty())
        .map(OsString::from_wide)
        .collect();
    let mut builder = ProcessEnvBuilder::new(inherited);
    process_env_builder_add_entry(&mut builder, key.to_owned(), value.to_owned());
    *env = finish_process_env_builder(builder);
}

#[cfg(unix)]
fn unix_process_env_entry(key: OsString, value: OsString) -> Option<OsString> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let key = key.as_os_str().as_bytes();
    if key.contains(&0) {
        return None;
    }
    let value = value.as_os_str().as_bytes();
    let value = &value[..value
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(value.len())];
    let mut entry = Vec::with_capacity(key.len() + value.len() + 1);
    entry.extend_from_slice(key);
    entry.push(b'=');
    entry.extend_from_slice(value);
    Some(OsString::from_vec(entry))
}

ported_fns! {
    #[ported(
        source = "src/process/unix.c",
        original = "moonbitlang_async_env_block_add_entry"
    )]
    #[cfg(unix)]
    pub(crate) fn process_env_builder_add_entry(
        builder: &mut ProcessEnvBuilder,
        key: OsString,
        value: OsString,
    ) {
        if let Some(entry) = unix_process_env_entry(key, value) {
            builder.extra.push(entry);
        }
    }

    #[ported(
        source = "src/process/windows.c",
        original = "moonbitlang_async_env_block_add_entry"
    )]
    #[cfg(windows)]
    pub(crate) fn process_env_builder_add_entry(
        builder: &mut ProcessEnvBuilder,
        mut key: OsString,
        value: OsString,
    ) {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};

        if key.encode_wide().any(|unit| unit == 0) {
            return;
        }
        let value = value
            .encode_wide()
            .take_while(|unit| *unit != 0)
            .collect::<Vec<_>>();
        key.push("=");
        key.push(OsString::from_wide(&value));
        builder.extra.push(key);
    }

    #[ported(
        source = "src/process/unix.c",
        original = "moonbitlang_async_write_env_block"
    )]
    #[cfg(unix)]
    pub(crate) fn finish_process_env_builder(mut builder: ProcessEnvBuilder) -> Vec<OsString> {
        use std::os::unix::ffi::OsStrExt;

        let extra_len = builder.extra.len();
        let inherited = builder
            .inherited
            .into_iter()
            .filter(|entry| {
                let entry = entry.as_os_str().as_bytes();
                let duplicate = if let Some(key_end) = entry.iter().position(|byte| *byte == b'=')
                {
                    builder.extra[..extra_len].iter().any(|extra| {
                        extra
                            .as_os_str()
                            .as_bytes()
                            .get(..=key_end)
                            .is_some_and(|key| key == &entry[..=key_end])
                    })
                } else {
                    builder.extra[..extra_len]
                        .iter()
                        .any(|extra| extra.as_os_str().as_bytes() == entry)
                };
                !duplicate
            })
            .collect::<Vec<_>>();
        builder.extra.extend(inherited);
        builder.extra
    }

    #[ported(
        source = "src/process/windows.c",
        original = "moonbitlang_async_write_env_block"
    )]
    #[cfg(windows)]
    pub(crate) fn finish_process_env_builder(builder: ProcessEnvBuilder) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};

        let extra_keys = builder
            .extra
            .iter()
            .filter_map(|entry| {
                let entry = entry.encode_wide().collect::<Vec<_>>();
                let key_len = entry.iter().position(|unit| *unit == b'=' as u16)?;
                Some(entry[..key_len].to_vec())
            })
            .collect::<Vec<_>>();
        let inherited = builder
            .inherited
            .into_iter()
            .filter(|entry| {
                let entry = entry.encode_wide().collect::<Vec<_>>();
                let Some(key_len) = entry.iter().position(|unit| *unit == b'=' as u16) else {
                    return false;
                };
                if key_len == 0 {
                    return false;
                }
                !extra_keys.iter().any(|extra_key| {
                    key_len == extra_key.len()
                        && unsafe {
                            CompareStringOrdinal(
                                extra_key.as_ptr(),
                                extra_key.len() as i32,
                                entry.as_ptr(),
                                key_len as i32,
                                1,
                            )
                        } == CSTR_EQUAL
                })
            })
            .collect::<Vec<_>>();

        // Create the one contiguous block only when ownership moves to the
        // spawn job. Until this point the builder contains native OsStrings.
        let mut block = Vec::new();
        for entry in builder.extra.into_iter().chain(inherited) {
            block.extend(entry.encode_wide());
            block.push(0);
        }
        if block.is_empty() {
            block.push(0);
        }
        block.push(0);
        block
    }

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
                return Ok(unix_siginfo_exit_code(
                    info.si_code,
                    unsafe { info.si_status() },
                ));
            }

            let mut status = 0;
            let ret = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
            if ret < 0 {
                return Err(last_native_error());
            }
            if ret == 0 {
                return Err(AsyncHostError::Native(libc::EAGAIN));
            }
            Ok(unix_wait_status_exit_code(status))
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
pub(crate) fn unix_siginfo_exit_code(code: i32, status: i32) -> i32 {
    if code == libc::CLD_EXITED {
        status
    } else {
        -status
    }
}

#[cfg(unix)]
pub(crate) fn unix_wait_status_exit_code(status: i32) -> i32 {
    if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else {
        -libc::WTERMSIG(status)
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

#[cfg(all(test, unix))]
mod tests {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    use super::*;

    #[test]
    fn unix_process_env_discards_nul_keys_and_truncates_nul_values() {
        let mut builder = ProcessEnvBuilder::new(Vec::new());
        process_env_builder_add_entry(
            &mut builder,
            OsString::from("BAD\0INJECTED"),
            OsString::from("value"),
        );
        process_env_builder_add_entry(
            &mut builder,
            OsString::from("GOOD"),
            OsString::from("before\0INJECTED=value"),
        );

        assert_eq!(
            finish_process_env_builder(builder),
            vec![OsString::from("GOOD=before")]
        );
    }

    #[test]
    fn unix_process_env_overwrite_replaces_guest_value_and_preserves_bytes() {
        let mut env = vec![
            OsString::from("MOONRUN_INHERITED_POLICY=guest"),
            OsString::from("KEEP=value"),
        ];
        let value = OsString::from_vec(b"/tmp/policy-\xFF.json".to_vec());

        overwrite_process_env_var(&mut env, OsStr::new("MOONRUN_INHERITED_POLICY"), &value);

        assert_eq!(
            env,
            vec![
                OsString::from_vec(b"MOONRUN_INHERITED_POLICY=/tmp/policy-\xFF.json".to_vec()),
                OsString::from("KEEP=value"),
            ]
        );
    }

    #[test]
    fn unix_wait_status_distinguishes_exit_from_signal() {
        assert_eq!(unix_wait_status_exit_code(7 << 8), 7);
        assert_eq!(unix_wait_status_exit_code(libc::SIGTERM), -libc::SIGTERM);
    }

    #[test]
    fn unix_siginfo_distinguishes_exit_from_signal() {
        assert_eq!(unix_siginfo_exit_code(libc::CLD_EXITED, 7), 7);
        assert_eq!(
            unix_siginfo_exit_code(libc::CLD_KILLED, libc::SIGTERM),
            -libc::SIGTERM
        );
    }
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
    use std::ffi::{OsStr, OsString};
    use std::os::windows::io::AsRawHandle;
    use std::process::Command;

    use super::*;

    #[test]
    fn windows_process_env_discards_nul_keys_and_truncates_nul_values() {
        let mut builder = ProcessEnvBuilder::new(Vec::new());
        process_env_builder_add_entry(
            &mut builder,
            OsString::from("BAD\0INJECTED"),
            OsString::from("value"),
        );
        process_env_builder_add_entry(
            &mut builder,
            OsString::from("GOOD"),
            OsString::from("before\0INJECTED=value"),
        );

        assert_eq!(
            finish_process_env_builder(builder),
            "GOOD=before\0\0".encode_utf16().collect::<Vec<_>>()
        );
    }

    #[test]
    fn windows_process_env_overwrite_replaces_guest_value_case_insensitively() {
        let mut env = "moonrun_inherited_policy=guest\0KEEP=\u{6708}\0\0"
            .encode_utf16()
            .collect();

        overwrite_process_env_var(
            &mut env,
            OsStr::new("MOONRUN_INHERITED_POLICY"),
            OsStr::new("C:\\\u{6708}\\policy.json"),
        );

        assert_eq!(
            env,
            "MOONRUN_INHERITED_POLICY=C:\\\u{6708}\\policy.json\0KEEP=\u{6708}\0\0"
                .encode_utf16()
                .collect::<Vec<_>>()
        );
    }

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
