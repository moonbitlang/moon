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

//! Path-based mechanisms from async's fs.c at d53eadfc623d343edaf01f0b11dad28badcb1274.
//! Preserve native path and error semantics; callers own cwd resolution and authority.

use std::ffi::{OsStr, OsString};
use std::io;

#[cfg(unix)]
use super::path_to_cstring;
#[cfg(windows)]
use super::path_to_wide;

pub(in crate::filesystem) enum AccessMode {
    Exists,
    Read,
    Write,
    Execute,
}

#[cfg(unix)]
pub(in crate::filesystem) fn access(path: &OsStr, access: AccessMode) -> io::Result<()> {
    let path = path_to_cstring(path)?;
    let mode = match access {
        AccessMode::Exists => libc::F_OK,
        AccessMode::Read => libc::R_OK,
        AccessMode::Write => libc::W_OK,
        AccessMode::Execute => libc::X_OK,
    };
    if unsafe { libc::access(path.as_ptr(), mode) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
pub(in crate::filesystem) fn chmod(path: &OsStr, mode: u32) -> io::Result<()> {
    let path = path_to_cstring(path)?;
    if unsafe { libc::chmod(path.as_ptr(), mode as libc::mode_t) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
pub(in crate::filesystem) fn remove(path: &OsStr) -> io::Result<()> {
    let path = path_to_cstring(path)?;
    if unsafe { libc::remove(path.as_ptr()) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
pub(in crate::filesystem) fn mkdir(path: &OsStr, mode: u32) -> io::Result<()> {
    let path = path_to_cstring(path)?;
    if unsafe { libc::mkdir(path.as_ptr(), mode as libc::mode_t) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
pub(in crate::filesystem) fn rmdir(path: &OsStr) -> io::Result<()> {
    let path = path_to_cstring(path)?;
    if unsafe { libc::rmdir(path.as_ptr()) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
pub(in crate::filesystem) fn realpath(path: &OsStr) -> io::Result<OsString> {
    let path = path_to_cstring(path)?;
    let resolved = unsafe { libc::realpath(path.as_ptr(), std::ptr::null_mut()) };
    if resolved.is_null() {
        return Err(io::Error::last_os_error());
    }

    use std::os::unix::ffi::OsStringExt;
    let path = OsString::from_vec(
        unsafe { std::ffi::CStr::from_ptr(resolved) }
            .to_bytes()
            .to_vec(),
    );
    #[cfg(target_os = "macos")]
    unsafe {
        unsafe extern "C" {
            fn malloc_zone_from_ptr(ptr: *const std::ffi::c_void) -> *mut std::ffi::c_void;
            fn malloc_zone_free(zone: *mut std::ffi::c_void, ptr: *mut std::ffi::c_void);
        }

        // realpath may allocate from a non-default malloc zone on macOS.
        let zone = malloc_zone_from_ptr(resolved.cast());
        malloc_zone_free(zone, resolved.cast());
    }
    #[cfg(not(target_os = "macos"))]
    unsafe {
        libc::free(resolved.cast());
    }
    Ok(path)
}

#[cfg(windows)]
pub(in crate::filesystem) fn access(path: &OsStr, access: AccessMode) -> io::Result<()> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_EXECUTE, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let access_mode = match access {
        AccessMode::Exists => 0,
        AccessMode::Read => GENERIC_READ,
        AccessMode::Write => GENERIC_WRITE,
        AccessMode::Execute => FILE_EXECUTE,
    };
    let path = path_to_wide(path);
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access_mode,
            FILE_SHARE_DELETE | FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        unsafe {
            CloseHandle(handle);
        }
        Ok(())
    }
}

#[cfg(windows)]
pub(in crate::filesystem) fn chmod(_path: &OsStr, _mode: u32) -> io::Result<()> {
    Err(io::Error::from_raw_os_error(
        windows_sys::Win32::Foundation::ERROR_NOT_SUPPORTED as i32,
    ))
}

#[cfg(windows)]
pub(in crate::filesystem) fn remove(path: &OsStr) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        DeleteFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, GetFileAttributesW,
        INVALID_FILE_ATTRIBUTES, RemoveDirectoryW,
    };

    let path = path_to_wide(path);
    let attrs = unsafe { GetFileAttributesW(path.as_ptr()) };
    if attrs == INVALID_FILE_ATTRIBUTES {
        return Err(io::Error::last_os_error());
    }

    let is_directory_link =
        (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0 && (attrs & FILE_ATTRIBUTE_REPARSE_POINT) != 0;
    let ok = if is_directory_link {
        // Windows removes directory symlinks and junctions through RemoveDirectoryW,
        // not DeleteFileW, even though they are reparse points rather than real dirs.
        unsafe { RemoveDirectoryW(path.as_ptr()) }
    } else {
        unsafe { DeleteFileW(path.as_ptr()) }
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub(in crate::filesystem) fn mkdir(path: &OsStr, _mode: u32) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;

    let path = path_to_wide(path);
    if unsafe { CreateDirectoryW(path.as_ptr(), std::ptr::null()) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub(in crate::filesystem) fn rmdir(path: &OsStr) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::RemoveDirectoryW;

    let path = path_to_wide(path);
    if unsafe { RemoveDirectoryW(path.as_ptr()) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub(in crate::filesystem) fn realpath(path: &OsStr) -> io::Result<OsString> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_INVALID_ADDRESS, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_NAME_NORMALIZED,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFinalPathNameByHandleW,
        OPEN_EXISTING, VOLUME_NAME_DOS,
    };

    const BUFFER_LEN: usize = 1024;

    let path = path_to_wide(path);
    let file = unsafe {
        CreateFileW(
            path.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if file == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    let result = (|| {
        let flags = FILE_NAME_NORMALIZED | VOLUME_NAME_DOS;
        let mut stack_buffer = [0u16; BUFFER_LEN];
        let len = unsafe {
            GetFinalPathNameByHandleW(file, stack_buffer.as_mut_ptr(), BUFFER_LEN as u32, flags)
        };
        if len == 0 {
            return Err(io::Error::last_os_error());
        }

        let len = usize::try_from(len)
            .map_err(|_| io::Error::from_raw_os_error(ERROR_INVALID_ADDRESS as i32))?;
        if len < stack_buffer.len() {
            return Ok(windows_realpath(&stack_buffer[..len]));
        }

        let buffer_len = len
            .checked_add(1)
            .ok_or(io::Error::from_raw_os_error(ERROR_INVALID_ADDRESS as i32))?;
        let mut heap_buffer = vec![0u16; buffer_len];
        let len = unsafe {
            GetFinalPathNameByHandleW(
                file,
                heap_buffer.as_mut_ptr(),
                u32::try_from(heap_buffer.len())
                    .map_err(|_| io::Error::from_raw_os_error(ERROR_INVALID_ADDRESS as i32))?,
                flags,
            )
        };
        if len == 0 {
            return Err(io::Error::last_os_error());
        }
        let len = usize::try_from(len)
            .map_err(|_| io::Error::from_raw_os_error(ERROR_INVALID_ADDRESS as i32))?;
        let units = heap_buffer
            .get(..len)
            .ok_or(io::Error::from_raw_os_error(ERROR_INVALID_ADDRESS as i32))?;
        Ok(windows_realpath(units))
    })();

    unsafe {
        CloseHandle(file);
    }
    result
}

#[cfg(windows)]
fn windows_realpath(units: &[u16]) -> OsString {
    use std::os::windows::ffi::OsStringExt;
    const VERBATIM_PREFIX: [u16; 4] = ['\\' as u16, '\\' as u16, '?' as u16, '\\' as u16];
    const VERBATIM_UNC_PREFIX: [u16; 8] = [
        '\\' as u16,
        '\\' as u16,
        '?' as u16,
        '\\' as u16,
        'U' as u16,
        'N' as u16,
        'C' as u16,
        '\\' as u16,
    ];

    // Strip an extended prefix only when ordinary DOS parsing preserves every
    // component. Otherwise the returned path could name a different file.
    if units.starts_with(&VERBATIM_UNC_PREFIX) && windows_dos_path_is_exact(&units[8..]) {
        let mut normalized = units[6..].to_vec();
        normalized[0] = '\\' as u16;
        OsString::from_wide(&normalized)
    } else if units.starts_with(&VERBATIM_PREFIX)
        && !units.starts_with(&VERBATIM_UNC_PREFIX)
        && windows_dos_path_is_exact(&units[4..])
    {
        OsString::from_wide(&units[4..])
    } else {
        OsString::from_wide(units)
    }
}

#[cfg(windows)]
fn windows_dos_path_is_exact(units: &[u16]) -> bool {
    use windows_sys::Wdk::Storage::FileSystem::RtlIsDosDeviceName_U;
    use windows_sys::Win32::Foundation::MAX_PATH;
    if units.len() >= MAX_PATH as usize {
        return false;
    }
    let mut path = units.to_vec();
    path.push(0);
    let mut start = 0;
    while start < units.len() {
        let end = (start..units.len())
            .find(|&i| path[i] == '\\' as u16)
            .unwrap_or(units.len());
        if end == start || (path[end - 1] == u16::from(b'.') || path[end - 1] == u16::from(b' ')) {
            return false;
        }
        // The NT routine accepts a NUL-terminated component. Restore separators
        // before inspecting the next component, just as upstream does.
        path[end] = 0;
        let device = unsafe { RtlIsDosDeviceName_U(path[start..].as_ptr()) };
        if end < units.len() {
            path[end] = '\\' as u16;
        }
        if device != 0 {
            return false;
        }
        start = end + 1;
    }
    true
}

/// Query the native Windows temporary directory without selecting Runtime Env.
#[cfg(windows)]
pub(in crate::filesystem) fn temp_dir() -> io::Result<OsString> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::{ERROR_INVALID_ADDRESS, MAX_PATH};
    use windows_sys::Win32::Storage::FileSystem::GetTempPath2W;

    let mut buffer = [0u16; MAX_PATH as usize + 1];
    let len = unsafe { GetTempPath2W(buffer.len() as u32, buffer.as_mut_ptr()) };
    if len == 0 {
        return Err(io::Error::last_os_error());
    }
    let units = buffer
        .get(..len as usize)
        .ok_or_else(|| io::Error::from_raw_os_error(ERROR_INVALID_ADDRESS as i32))?;
    Ok(OsString::from_wide(units))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn realpath_preserves_windows_path_identity() {
        let normalize = |path: &str| {
            let units = path.encode_utf16().collect::<Vec<_>>();
            windows_realpath(&units).into_string().unwrap()
        };

        assert_eq!(normalize(r"\\?\C:\dir\file"), r"C:\dir\file");
        assert_eq!(
            normalize(r"\\?\UNC\server\share\file"),
            r"\\server\share\file"
        );
        assert_eq!(normalize(r"C:\already-normal"), r"C:\already-normal");
        // Bare device names remain reserved across Windows versions. Names with
        // extensions, such as NUL.txt, stopped being reserved in Windows 11.
        for path in [
            r"\\?\C:\dir.\file",
            r"\\?\C:\dir \file",
            r"\\?\C:\NUL",
            r"\\?\C:\COM¹",
            r"\\?\UNC\server\share\dir.\file",
            r"\\?\UNC\server\share\file ",
            r"\\?\UNC\server\share\NUL",
        ] {
            assert_eq!(normalize(path), path);
        }
        // Match upstream's MAX_PATH boundary, measured in UTF-16 code units.
        for filename in ["a".repeat(252), "😀".repeat(126)] {
            let short = format!(r"C:\dir\{filename}");
            assert_eq!(normalize(&format!(r"\\?\{short}")), short);
            let long = format!(r"\\?\{short}a");
            assert_eq!(normalize(&long), long);
        }
    }
}
