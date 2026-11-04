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
                let duplicate = if let Some(key_end) = entry.iter().position(|byte| *byte == b'=') {
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
    pub(crate) fn open_pid_handle(
        pid: i32,
    ) -> AsyncHostResult<crate::async_sys::internal::fd_util::stub::RawFd> {
        use std::os::fd::IntoRawFd;
        Ok(crate::process::os::open_pid_handle(pid)?.into_raw_fd())
    }

    #[ported(
        source = "src/internal/event_loop/process.c",
        original = "moonbitlang_async_open_pid_handle"
    )]
    #[cfg(windows)]
    pub(crate) fn open_pid_handle(
        pid: i32,
    ) -> AsyncHostResult<crate::async_sys::internal::fd_util::stub::RawFd> {
        use std::os::windows::io::IntoRawHandle;
        Ok(crate::process::os::open_pid_handle(pid)?.into_raw_handle())
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
        if handle.is_none() {
            return Err(AsyncHostError::Badf);
        }
        match crate::process::os::try_wait(handle, pid)? {
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
        crate::process::os::signal(pid, signal).map_err(Into::into)
    }

    #[ported(
        source = "src/process/unix.c",
        original = "moonbitlang_async_kill_process"
    )]
    #[cfg(unix)]
    pub(crate) fn kill_process(pid: i32) -> AsyncHostResult<()> {
        crate::process::os::kill(pid).map_err(Into::into)
    }

    #[ported(
        source = "src/process/windows.c",
        original = "moonbitlang_async_terminate_process"
    )]
    #[cfg(windows)]
    pub(crate) fn terminate_process(pid: i32, _signal: i32) -> AsyncHostResult<()> {
        // The native async ABI ignores failures from graceful cancellation.
        let _ = crate::process::os::signal(pid, _signal);
        Ok(())
    }

    #[ported(
        source = "src/process/windows.c",
        original = "moonbitlang_async_kill_process"
    )]
    #[cfg(windows)]
    pub(crate) fn kill_process(pid: i32) -> AsyncHostResult<()> {
        crate::process::os::kill(pid).map_err(Into::into)
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn pidfd_open_is_unsupported(error: AsyncHostError) -> bool {
    matches!(error, AsyncHostError::Native(errno) if errno == libc::ENOSYS || errno == libc::EPERM)
}

#[cfg(all(test, unix))]
mod tests {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::process::ExitStatusExt;

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
