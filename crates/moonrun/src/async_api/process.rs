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

#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use crate::async_host::{
    AsyncHostError, AsyncHostResult, GuestMemory, INVALID_HOST_HANDLE, read_u16,
};
use crate::async_sys::process;

use super::context::ImportContext;
use super::provenance::ported_imports;

ported_imports! {
#[ported(source = "src/process/unix.c", original = "moonbitlang_async_get_curr_env")]
#[cfg(unix)]
pub(super) fn get_curr_env(context: &mut ImportContext<'_, '_>) -> u64 {
    let env = if context.host.policy().has_env_policy() {
        context
            .host
            .policy()
            .env_vars()
            .into_iter()
            .map(|(key, value)| Some(OsString::from(format!("{key}={value}"))))
            .collect()
    } else {
        current_unix_env()
    };
    context.host.insert_process_env(env)
}

#[ported(source = "src/process/windows.c", original = "moonbitlang_async_get_curr_env")]
#[cfg(windows)]
pub(super) fn get_curr_env(context: &mut ImportContext<'_, '_>) -> AsyncHostResult<u64> {
    let env = if context.host.policy().has_env_policy() {
        let mut block = Vec::new();
        for (key, value) in context.host.policy().env_vars() {
            block.extend(format!("{key}={value}").encode_utf16());
            block.push(0);
        }
        if block.is_empty() {
            block.push(0);
        }
        block.push(0);
        block
    } else {
        current_windows_env()?
    };
    Ok(context.host.insert_process_env(env))
}

#[ported(source = "src/process/unix.c", original = "moonbitlang_async_env_block_length")]
#[cfg(unix)]
pub(super) fn env_block_length(
    context: &mut ImportContext<'_, '_>,
    env: u64,
) -> AsyncHostResult<i32> {
    context.host.process_env_length(env)
}

#[ported(source = "src/process/windows.c", original = "moonbitlang_async_env_block_length")]
#[cfg(windows)]
pub(super) fn env_block_length(
    context: &mut ImportContext<'_, '_>,
    env: u64,
) -> AsyncHostResult<i32> {
    context.host.process_env_length(env)
}

#[ported(source = "src/process/unix.c", original = "moonbitlang_async_allocate_env_block")]
#[cfg(unix)]
pub(super) fn allocate_env_block(
    context: &mut ImportContext<'_, '_>,
    size: i32,
) -> AsyncHostResult<u64> {
    let size = usize::try_from(size).map_err(|_| AsyncHostError::Fault)?;
    Ok(context.host.insert_process_env(vec![None; size]))
}

#[ported(source = "src/process/windows.c", original = "moonbitlang_async_allocate_env_block")]
#[cfg(windows)]
pub(super) fn allocate_env_block(
    context: &mut ImportContext<'_, '_>,
    size: i32,
) -> AsyncHostResult<u64> {
    let size = usize::try_from(size).map_err(|_| AsyncHostError::Fault)?;
    let size = size.checked_add(1).ok_or(AsyncHostError::Fault)?;
    Ok(context.host.insert_process_env(vec![0; size]))
}

pub(super) fn free_env(context: &mut ImportContext<'_, '_>, env: u64) -> AsyncHostResult<()> {
    context.host.free_process_env(env)
}

#[cfg(unix)]
pub(super) fn free_argv(context: &mut ImportContext<'_, '_>, argv: u64) -> AsyncHostResult<()> {
    context.host.free_process_argv(argv)
}

#[ported(source = "src/process/unix.c", original = "moonbitlang_async_write_env_block")]
#[cfg(unix)]
pub(super) fn write_env_block(
    context: &mut ImportContext<'_, '_>,
    dst: u64,
    src: u64,
) -> AsyncHostResult<()> {
    context.host.process_env_write_block(dst, src)
}

#[ported(source = "src/process/windows.c", original = "moonbitlang_async_write_env_block")]
#[cfg(windows)]
pub(super) fn write_env_block(
    context: &mut ImportContext<'_, '_>,
    dst: u64,
    src: u64,
) -> AsyncHostResult<()> {
    context.host.process_env_write_block(dst, src)
}

#[ported(source = "src/process/unix.c", original = "moonbitlang_async_env_block_add_entry")]
#[cfg(unix)]
pub(super) fn env_block_add_entry(
    context: &mut ImportContext<'_, '_>,
    env: u64,
    index: i32,
    key: i32,
    key_len: i32,
    value: i32,
    value_len: i32,
) -> AsyncHostResult<()> {
    let entry = unix_env_entry(context, key, key_len, value, value_len)?;
    context.host.process_env_add_entry(env, index, entry)
}

#[ported(source = "src/process/windows.c", original = "moonbitlang_async_env_block_add_entry")]
#[cfg(windows)]
pub(super) fn env_block_add_entry(
    context: &mut ImportContext<'_, '_>,
    env: u64,
    offset: i32,
    key: i32,
    key_len: i32,
    value: i32,
    value_len: i32,
) -> AsyncHostResult<()> {
    let key = read_guest_u16(context, key, key_len)?;
    let value = read_guest_u16(context, value, value_len)?;
    context
        .host
        .process_env_add_entry(env, offset, &key, &value)
}

#[ported(source = "src/process/unix.c", original = "moonbitlang_async_make_argv_array")]
#[cfg(unix)]
pub(super) fn make_argv_array_unix(
    context: &mut ImportContext<'_, '_>,
    len: i32,
) -> AsyncHostResult<u64> {
    context.host.insert_process_argv(len)
}

#[ported(
    source = "src/process/unix.c",
    original = "moonbitlang_async_argv_array_add_encoded_entry"
)]
#[cfg(unix)]
pub(super) fn argv_array_add_encoded_entry_unix(
    context: &mut ImportContext<'_, '_>,
    argv: u64,
    index: i32,
    arg: i32,
    arg_len: i32,
) -> AsyncHostResult<()> {
    let arg = read_guest_os_string(context, arg, arg_len)?;
    context.host.process_argv_add_entry(argv, index, arg)
}

#[ported(source = "src/process/unix.c", original = "moonbitlang_async_argv_array_add_entry")]
#[cfg(unix)]
pub(super) fn argv_array_add_entry_unix(
    context: &mut ImportContext<'_, '_>,
    argv: u64,
    index: i32,
    arg: i32,
    arg_len: i32,
) -> AsyncHostResult<()> {
    let arg = read_guest_os_string(context, arg, arg_len)?;
    context.host.process_argv_add_entry(argv, index, arg)
}

#[cfg(all(unix, not(target_os = "linux")))]
pub(super) fn open_pid_handle(
    context: &mut ImportContext<'_, '_>,
    pid: i32,
) -> AsyncHostResult<u64> {
    let handle = context
        .host
        .with_owned_child_pid(pid, || Ok(context.host.invalid_fd()))?;
    // The invalid handle selects the kqueue fallback rather than reporting an
    // error. Match the native C implementation by clearing any stale errno.
    context.host.set_errno(0);
    Ok(handle)
}

#[ported(
    source = "src/internal/event_loop/process.c",
    original = "moonbitlang_async_open_pid_handle"
)]
#[cfg(target_os = "linux")]
pub(super) fn open_pid_handle(
    context: &mut ImportContext<'_, '_>,
    pid: i32,
) -> AsyncHostResult<u64> {
    let handle = context.host.with_owned_child_pid(pid, || {
        match process::open_pid_handle(pid) {
            Ok(handle) => Ok(Some(handle)),
            Err(error) if process::pidfd_open_is_unsupported(error) => Ok(None),
            Err(error) => Err(error),
        }
    })?;
    match handle {
        Some(handle) => Ok(context.host.insert_host_process_handle(handle, pid)),
        None => Ok(context.host.invalid_fd()),
    }
}

#[ported(
    source = "src/internal/event_loop/process.c",
    original = "moonbitlang_async_open_pid_handle"
)]
#[cfg(windows)]
pub(super) fn open_pid_handle(
    context: &mut ImportContext<'_, '_>,
    pid: i32,
) -> AsyncHostResult<u64> {
    let handle = context
        .host
        .with_owned_child_pid(pid, || process::open_pid_handle(pid))?;
    Ok(context.host.insert_host_process_handle(handle, pid))
}

#[ported(
    source = "src/internal/event_loop/process.c",
    original = "moonbitlang_async_get_process_result"
)]
pub(super) fn get_process_result(
    context: &mut ImportContext<'_, '_>,
    handle: u64,
    pid: i32,
    out: i32,
) -> i32 {
    let result = (|| {
        let handle_id = if handle == INVALID_HOST_HANDLE || handle == context.host.invalid_fd() {
            None
        } else {
            Some(handle)
        };
        let resource = handle_id
            .map(|handle| context.host.resource(handle))
            .transpose()?;
        #[cfg(unix)]
        let raw_handle = resource
            .as_ref()
            .map(|resource| resource.as_fd().map(|fd| fd.as_raw_fd()))
            .transpose()?;
        #[cfg(windows)]
        let raw_handle = resource
            .as_ref()
            .map(|resource| resource.as_handle().map(|handle| handle.as_raw_handle()))
            .transpose()?;
        #[cfg(unix)]
        let code = context.host.finish_owned_child(pid, handle_id, || {
            process::get_process_result(raw_handle, pid)
        })?;
        #[cfg(windows)]
        let code = {
            let handle_id = handle_id.ok_or(AsyncHostError::Badf)?;
            let raw_handle = raw_handle.ok_or(AsyncHostError::Badf)?;
            context.host.finish_process_handle(pid, handle_id, || {
                if context.host.policy().has_process_policy()
                    && process::process_id_from_handle(raw_handle)? != pid
                {
                    return Err(AsyncHostError::PermissionDenied);
                }
                process::get_process_result(Some(raw_handle), pid)
            })?
        };
        context.with_memory_mut(|memory| memory.write_exact(out, &code.to_le_bytes()))
    })();
    match result {
        Ok(()) => 0,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(
    source = "src/process/unix.c",
    original = "moonbitlang_async_terminate_process"
)]
#[cfg(unix)]
pub(super) fn terminate(
    context: &mut ImportContext<'_, '_>,
    pid: i32,
    signal: i32,
) -> AsyncHostResult<()> {
    context.host.with_owned_child_pid(pid, || {
        let _ = process::terminate_process(pid, signal);
        Ok(())
    })
}

#[ported(
    source = "src/process/windows.c",
    original = "moonbitlang_async_terminate_process"
)]
#[cfg(windows)]
pub(super) fn terminate(
    context: &mut ImportContext<'_, '_>,
    pid: i32,
    signal: i32,
) -> AsyncHostResult<()> {
    context.host.with_owned_child_pid(pid, || {
        let _ = process::terminate_process(pid, signal);
        Ok(())
    })
}

#[ported(
    source = "src/process/unix.c",
    original = "moonbitlang_async_kill_process"
)]
#[cfg(unix)]
pub(super) fn kill(context: &mut ImportContext<'_, '_>, pid: i32) -> AsyncHostResult<()> {
    context.host.with_owned_child_pid(pid, || {
        let _ = process::kill_process(pid);
        Ok(())
    })
}

#[ported(
    source = "src/process/windows.c",
    original = "moonbitlang_async_kill_process"
)]
#[cfg(windows)]
pub(super) fn kill(context: &mut ImportContext<'_, '_>, pid: i32) -> AsyncHostResult<()> {
    context.host.with_owned_child_pid(pid, || {
        let _ = process::kill_process(pid);
        Ok(())
    })
}
}

#[cfg(unix)]
fn current_unix_env() -> Vec<Option<OsString>> {
    use std::ffi::CStr;
    use std::os::unix::ffi::OsStringExt;

    unsafe extern "C" {
        static mut environ: *mut *mut libc::c_char;
    }

    let mut entries = Vec::new();
    let mut cursor = unsafe { environ };
    while !cursor.is_null() {
        let entry = unsafe { *cursor };
        if entry.is_null() {
            break;
        }
        entries.push(Some(OsString::from_vec(
            unsafe { CStr::from_ptr(entry) }.to_bytes().to_vec(),
        )));
        cursor = unsafe { cursor.add(1) };
    }
    entries
}

#[cfg(windows)]
fn current_windows_env() -> AsyncHostResult<Vec<u16>> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Environment::{
        FreeEnvironmentStringsW, GetEnvironmentStringsW,
    };

    let block = unsafe { GetEnvironmentStringsW() };
    if block.is_null() {
        return Err(AsyncHostError::Native(unsafe { GetLastError() } as i32));
    }

    let mut entries = Vec::new();
    let mut cursor = block;
    loop {
        let mut len = 0usize;
        while unsafe { *cursor.add(len) } != 0 {
            len += 1;
        }
        if len == 0 {
            break;
        }
        if unsafe { *cursor } != b'=' as u16 {
            let entry = unsafe { std::slice::from_raw_parts(cursor, len) };
            entries.extend_from_slice(entry);
            entries.push(0);
        }
        cursor = unsafe { cursor.add(len + 1) };
    }
    unsafe {
        FreeEnvironmentStringsW(block);
    }

    if entries.is_empty() {
        entries.push(0);
    }
    entries.push(0);
    Ok(entries)
}

#[cfg(unix)]
fn unix_env_entry(
    context: &mut ImportContext<'_, '_>,
    key: i32,
    key_len: i32,
    value: i32,
    value_len: i32,
) -> AsyncHostResult<OsString> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let key = read_guest_os_string(context, key, key_len)?;
    let value = read_guest_os_string(context, value, value_len)?;
    let mut entry = key.as_os_str().as_bytes().to_vec();
    entry.push(b'=');
    entry.extend_from_slice(value.as_os_str().as_bytes());
    Ok(OsString::from_vec(entry))
}

#[cfg(unix)]
fn read_guest_os_string(
    context: &mut ImportContext<'_, '_>,
    ptr: i32,
    len: i32,
) -> AsyncHostResult<OsString> {
    context.with_memory_mut(|memory| {
        let units = read_u16(memory, ptr, len)?;

        use std::os::unix::ffi::OsStringExt;

        let value = char::decode_utf16(units)
            .map(Result::unwrap)
            .collect::<String>();
        Ok(OsString::from_vec(value.into_bytes()))
    })
}

#[cfg(windows)]
fn read_guest_u16(
    context: &mut ImportContext<'_, '_>,
    ptr: i32,
    len: i32,
) -> AsyncHostResult<Vec<u16>> {
    context.with_memory_mut(|memory| read_u16(memory, ptr, len))
}
