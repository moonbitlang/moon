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

use std::ffi::{OsStr, OsString};

#[cfg(windows)]
use crate::async_host::AsyncHostError;
use crate::async_host::AsyncHostResult;
use crate::async_sys::ported_fns;

ported_fns! {
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
    tmp.and_then(separator_terminated_windows_path)
        .or_else(|| temp.and_then(separator_terminated_windows_path))
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
}
