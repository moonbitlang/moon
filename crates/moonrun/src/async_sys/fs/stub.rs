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

use std::ffi::{OsStr, OsString};
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::event_loop::thread_pool::Resource;
use crate::async_sys::ported_fns;

#[cfg(unix)]
pub(crate) type RawFileHandle = std::os::fd::RawFd;

#[cfg(windows)]
pub(crate) type RawFileHandle = windows_sys::Win32::Foundation::HANDLE;

ported_fns! {
    #[ported(
        source = "src/fs/stub.c",
        original = "moonbitlang_async_errno_is_lock_violation"
    )]
    pub(crate) fn errno_is_lock_violation(errno: i32) -> bool {
        #[cfg(unix)]
        {
            errno == libc::EWOULDBLOCK
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::ERROR_LOCK_VIOLATION;
            errno == ERROR_LOCK_VIOLATION as i32
        }
    }

    #[ported(
        source = "src/fs/stub.c",
        original = "moonbitlang_async_get_tmp_path"
    )]
    pub(crate) fn get_tmp_path() -> AsyncHostResult<OsString> {
        #[cfg(unix)]
        {
            Ok(tmp_path_from_native_stub())
        }
        #[cfg(windows)]
        {
            tmp_path_from_native_stub()
        }
    }

    #[ported(
        source = "src/fs/stub.c",
        original = "moonbitlang_async_try_lock_file"
    )]
    pub(crate) fn try_lock_file(handle: RawFileHandle, exclusive: bool) -> AsyncHostResult<()> {
        try_lock_file_from_native_stub(handle, exclusive)
    }

    #[ported(
        source = "src/fs/stub.c",
        original = "moonbitlang_async_unlock_file"
    )]
    pub(crate) fn unlock_file(handle: RawFileHandle) -> AsyncHostResult<()> {
        unlock_file_from_native_stub(handle)
    }
}

#[cfg(unix)]
fn try_lock_file_from_native_stub(fd: RawFileHandle, exclusive: bool) -> AsyncHostResult<()> {
    let operation = libc::LOCK_NB
        | if exclusive {
            libc::LOCK_EX
        } else {
            libc::LOCK_SH
        };
    if unsafe { libc::flock(fd, operation) } == 0 {
        Ok(())
    } else {
        Err(last_native_error())
    }
}

#[cfg(unix)]
fn unlock_file_from_native_stub(fd: RawFileHandle) -> AsyncHostResult<()> {
    if unsafe { libc::flock(fd, libc::LOCK_UN) } == 0 {
        Ok(())
    } else {
        Err(last_native_error())
    }
}

#[cfg(windows)]
fn try_lock_file_from_native_stub(handle: RawFileHandle, exclusive: bool) -> AsyncHostResult<()> {
    use std::mem::zeroed;
    use windows_sys::Win32::Storage::FileSystem::{
        LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx,
    };
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let mut overlapped: OVERLAPPED = unsafe { zeroed() };
    // Keep parity with async's native stub: lock a one-byte sentinel range at
    // the end of the 64-bit file-position space.
    overlapped.Anonymous.Anonymous.Offset = 0xfffffffe;
    overlapped.Anonymous.Anonymous.OffsetHigh = 0xffffffff;
    let flags = LOCKFILE_FAIL_IMMEDIATELY
        | if exclusive {
            LOCKFILE_EXCLUSIVE_LOCK
        } else {
            0
        };

    if unsafe { LockFileEx(handle, flags, 0, 1, 0, &mut overlapped) } != 0 {
        Ok(())
    } else {
        Err(last_native_error())
    }
}

#[cfg(windows)]
fn unlock_file_from_native_stub(handle: RawFileHandle) -> AsyncHostResult<()> {
    use windows_sys::Win32::Storage::FileSystem::UnlockFile;

    if unsafe { UnlockFile(handle, 0xfffffffe, 0xffffffff, 1, 0) } != 0 {
        Ok(())
    } else {
        Err(last_native_error())
    }
}

pub(crate) fn try_lock_acquired_file(file: &Resource, exclusive: bool) -> AsyncHostResult<()> {
    #[cfg(unix)]
    let handle = file.as_file()?.as_raw_fd();
    #[cfg(windows)]
    let handle = file.as_file()?.as_raw_handle();
    try_lock_file(handle, exclusive)
}

pub(crate) fn unlock_acquired_file(file: &Resource) -> AsyncHostResult<()> {
    #[cfg(unix)]
    let handle = file.as_file()?.as_raw_fd();
    #[cfg(windows)]
    let handle = file.as_file()?.as_raw_handle();
    unlock_file(handle)
}

#[cfg(unix)]
pub(crate) fn get_tmp_path_from_env(tmpdir: Option<OsString>) -> AsyncHostResult<OsString> {
    Ok(tmp_path_from_env(tmpdir))
}

#[cfg(windows)]
pub(crate) fn get_tmp_path_from_env(
    tmp: Option<OsString>,
    temp: Option<OsString>,
) -> AsyncHostResult<OsString> {
    tmp.or(temp)
        .and_then(separator_terminated_windows_path)
        .ok_or(AsyncHostError::PermissionDenied)
}

#[cfg(unix)]
pub(crate) fn tmp_path_buffer(path: &OsStr) -> AsyncHostResult<Box<[u8]>> {
    use std::os::unix::ffi::OsStrExt;

    Ok(copy_c_string_bytes(path.as_bytes()))
}

#[cfg(windows)]
pub(crate) fn tmp_path_buffer(path: &OsStr) -> AsyncHostResult<Box<[u8]>> {
    use std::os::windows::ffi::OsStrExt;

    Ok(wide_units_to_tmp_path_buffer(
        &path.encode_wide().collect::<Vec<_>>(),
    ))
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}

#[cfg(unix)]
fn tmp_path_from_native_stub() -> OsString {
    tmp_path_from_env(std::env::var_os("TMPDIR"))
}

#[cfg(unix)]
fn tmp_path_from_env(tmpdir: Option<OsString>) -> OsString {
    // POSIX reserves TMPDIR for temporary-file placement. The async tmpdir
    // layer concatenates this base path with a generated name, so the host
    // normalizes the Unix base to include the separator.
    tmpdir
        .and_then(separator_terminated_unix_path)
        .unwrap_or_else(default_unix_tmp_path)
}

#[cfg(all(unix, target_os = "android"))]
fn default_unix_tmp_path() -> OsString {
    OsString::from("/data/local/tmp/")
}

#[cfg(all(unix, not(target_os = "android")))]
fn default_unix_tmp_path() -> OsString {
    OsString::from("/tmp/")
}

#[cfg(unix)]
fn separator_terminated_unix_path(path: OsString) -> Option<OsString> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let bytes = path.as_os_str().as_bytes();
    if bytes.is_empty() {
        return None;
    }
    if bytes.ends_with(b"/") {
        return Some(path);
    }

    let mut bytes = path.into_vec();
    bytes.push(b'/');
    Some(OsString::from_vec(bytes))
}

#[cfg(windows)]
fn separator_terminated_windows_path(path: OsString) -> Option<OsString> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    let mut units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if units.is_empty() {
        return None;
    }
    if !matches!(units.last(), Some(unit) if *unit == b'\\' as u16 || *unit == b'/' as u16) {
        units.push(b'\\' as u16);
    }
    Some(OsString::from_wide(&units))
}

#[cfg(windows)]
fn tmp_path_from_native_stub() -> AsyncHostResult<OsString> {
    use crate::async_host::AsyncHostError;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::{GetLastError, MAX_PATH};
    use windows_sys::Win32::Storage::FileSystem::GetTempPath2W;

    let mut buffer = [0u16; MAX_PATH as usize + 1];
    let len = unsafe { GetTempPath2W(buffer.len() as u32, buffer.as_mut_ptr()) };
    if len == 0 {
        return Err(AsyncHostError::Native(unsafe { GetLastError() as i32 }));
    }

    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    Ok(OsString::from_wide(&buffer[..len]))
}

#[cfg(unix)]
fn copy_c_string_bytes(bytes: &[u8]) -> Box<[u8]> {
    let mut buffer = Box::<[u8]>::new_uninit_slice(bytes.len() + 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.as_mut_ptr().cast(), bytes.len());
    }
    buffer[bytes.len()].write(0);
    unsafe { buffer.assume_init() }
}

#[cfg(windows)]
fn copy_wide_c_string_bytes(units: &[u16]) -> Box<[u8]> {
    let byte_len = std::mem::size_of_val(units);
    let mut buffer = Box::<[u8]>::new_uninit_slice(byte_len + std::mem::size_of::<u16>());
    unsafe {
        std::ptr::copy_nonoverlapping(
            units.as_ptr().cast::<u8>(),
            buffer.as_mut_ptr().cast(),
            byte_len,
        );
    }
    buffer[byte_len].write(0);
    buffer[byte_len + 1].write(0);
    unsafe { buffer.assume_init() }
}

#[cfg(windows)]
fn wide_units_to_tmp_path_buffer(units: &[u16]) -> Box<[u8]> {
    copy_wide_c_string_bytes(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmp_path_is_non_empty_and_separator_terminated() {
        let path = get_tmp_path().unwrap();

        assert!(!path.as_os_str().is_empty());
        let path = path.to_string_lossy();
        assert!(path.ends_with('/') || path.ends_with('\\'));
    }

    #[test]
    fn tmp_path_buffer_is_non_empty() {
        let path = get_tmp_path().unwrap();
        let buffer = tmp_path_buffer(path.as_os_str()).unwrap();

        assert!(!buffer.is_empty());
        #[cfg(unix)]
        assert_eq!(buffer.last(), Some(&0));
        #[cfg(windows)]
        assert_eq!(
            buffer.get(buffer.len().saturating_sub(2)..),
            Some(&[0, 0][..])
        );
    }

    #[cfg(all(unix, not(target_os = "android")))]
    #[test]
    fn default_unix_tmp_path_matches_async_stub_fallback() {
        use std::os::unix::ffi::OsStrExt;

        assert_eq!(default_unix_tmp_path().as_os_str().as_bytes(), b"/tmp/");
    }

    #[cfg(unix)]
    #[test]
    fn tmpdir_env_value_is_separator_terminated() {
        use std::os::unix::ffi::OsStrExt;

        assert_eq!(
            separator_terminated_unix_path(OsString::from("/var/tmp"))
                .unwrap()
                .as_os_str()
                .as_bytes(),
            b"/var/tmp/"
        );
    }

    #[cfg(unix)]
    #[test]
    fn empty_tmpdir_env_value_is_ignored() {
        assert_eq!(separator_terminated_unix_path(OsString::from("")), None);
    }

    #[cfg(unix)]
    #[test]
    fn unix_lock_violation_matches_async_stub() {
        assert!(errno_is_lock_violation(libc::EWOULDBLOCK));
        assert!(!errno_is_lock_violation(libc::EINVAL));
    }

    #[cfg(unix)]
    #[test]
    fn unix_try_lock_and_unlock_file_match_async_stub() {
        use std::os::fd::AsRawFd;

        let path =
            std::env::temp_dir().join(format!("moonrun-async-lock-test-{}", std::process::id()));
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();

        try_lock_file(file.as_raw_fd(), true).unwrap();
        unlock_file(file.as_raw_fd()).unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn unix_invalid_lock_file_records_native_errno() {
        assert_eq!(
            try_lock_file(-1, true),
            Err(AsyncHostError::Native(libc::EBADF))
        );
        assert_eq!(unlock_file(-1), Err(AsyncHostError::Native(libc::EBADF)));
    }
}
