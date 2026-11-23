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

//! Symlink and Windows junction mechanisms from async's fs.c at
//! d53eadfc623d343edaf01f0b11dad28badcb1274.

use std::ffi::OsStr;
use std::io;
#[cfg(windows)]
use std::{ffi::OsString, os::windows::ffi::OsStrExt};

#[cfg(unix)]
use super::path_to_cstring;
#[cfg(windows)]
use super::path_to_wide;

#[cfg(unix)]
pub(in crate::filesystem) fn create(
    target: &OsStr,
    path: &OsStr,
    _force_symlink: bool,
) -> io::Result<()> {
    let target = path_to_cstring(target)?;
    let path = path_to_cstring(path)?;
    if unsafe { libc::symlink(target.as_ptr(), path.as_ptr()) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub(in crate::filesystem) fn create(
    target: &OsStr,
    path: &OsStr,
    force_symlink: bool,
) -> io::Result<()> {
    use windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateSymbolicLinkW, SYMBOLIC_LINK_FLAG_DIRECTORY,
    };

    let target_wide = path_to_wide(target);
    let path_wide = path_to_wide(path);
    let is_directory = symlink_target_is_directory(target, path);

    if !force_symlink
        && is_directory
        && let Some(junction_target) = junction_target_path(target)
    {
        match create_junction(&junction_target, path) {
            Ok(()) => return Ok(()),
            Err(error) if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) => {}
            Err(error) => return Err(error),
        }
    }

    let flags = if is_directory {
        SYMBOLIC_LINK_FLAG_DIRECTORY
    } else {
        0
    };
    if unsafe { CreateSymbolicLinkW(path_wide.as_ptr(), target_wide.as_ptr(), flags) } {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn symlink_target_is_directory(target: &std::ffi::OsStr, path: &std::ffi::OsStr) -> bool {
    use std::path::{Component, Path};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, GetFileAttributesW, INVALID_FILE_ATTRIBUTES,
    };

    let target = Path::new(target);
    // Probe relative targets from the link's parent without changing the stored
    // link text. Drive-prefixed targets retain Windows' own path semantics.
    let probe = if matches!(target.components().next(), Some(Component::Prefix(_))) {
        target.to_owned()
    } else {
        Path::new(path)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(target)
    };
    let probe = path_to_wide(probe.as_os_str());
    let attrs = unsafe { GetFileAttributesW(probe.as_ptr()) };
    attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0
}

#[cfg(windows)]
fn create_junction(target: &OsStr, path: &OsStr) -> io::Result<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateDirectoryW, CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, RemoveDirectoryW,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;

    let data = junction_reparse_buffer(target)?;
    let path_wide = path_to_wide(path);
    if unsafe { CreateDirectoryW(path_wide.as_ptr(), std::ptr::null()) } == 0 {
        return Err(io::Error::last_os_error());
    }

    let handle = unsafe {
        CreateFileW(
            path_wide.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        let error = io::Error::last_os_error();
        unsafe {
            RemoveDirectoryW(path_wide.as_ptr());
        }
        return Err(error);
    }

    let mut bytes_returned = 0;
    let ok = unsafe {
        DeviceIoControl(
            handle,
            FSCTL_SET_REPARSE_POINT,
            data.as_ptr().cast(),
            data.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut bytes_returned,
            std::ptr::null_mut(),
        )
    };
    // Preserve the operation error across handle cleanup and rollback.
    let result = if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    };
    unsafe {
        CloseHandle(handle);
    }
    if let Err(error) = result {
        unsafe {
            RemoveDirectoryW(path_wide.as_ptr());
        }
        Err(error)
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn junction_target_path(target: &OsStr) -> Option<OsString> {
    use std::os::windows::ffi::OsStringExt;

    let mut target = target.encode_wide().collect::<Vec<_>>();
    const NT_PREFIX: [u16; 4] = ['\\' as u16, '?' as u16, '?' as u16, '\\' as u16];
    const WIN32_PREFIX: [u16; 4] = ['\\' as u16, '\\' as u16, '?' as u16, '\\' as u16];
    if target.starts_with(&NT_PREFIX) || target.starts_with(&WIN32_PREFIX) {
        target.drain(0..NT_PREFIX.len());
    }
    let drive = target.first().copied().unwrap_or_default();
    let is_drive_absolute = target.len() >= 3
        && (('a' as u16..='z' as u16).contains(&drive)
            || ('A' as u16..='Z' as u16).contains(&drive))
        && target[1] == ':' as u16
        && (target[2] == '\\' as u16 || target[2] == '/' as u16);
    is_drive_absolute.then(|| OsString::from_wide(&target))
}

#[cfg(windows)]
fn junction_reparse_buffer(target: &OsStr) -> io::Result<Vec<u8>> {
    use windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER;
    use windows_sys::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;

    const MOUNT_POINT_HEADER_LEN: usize = 8;
    const WCHAR_SIZE: usize = 2;
    const UNICODE_NULL_SIZE: usize = WCHAR_SIZE;
    const NON_INTERPRETED_PATH_PREFIX: [u16; 4] =
        ['\\' as u16, '?' as u16, '?' as u16, '\\' as u16];

    let mut print_name = target.encode_wide().collect::<Vec<_>>();
    if print_name.starts_with(&NON_INTERPRETED_PATH_PREFIX)
        || print_name.starts_with(&['\\' as u16, '\\' as u16, '?' as u16, '\\' as u16])
    {
        print_name.drain(0..NON_INTERPRETED_PATH_PREFIX.len());
    }
    for code_unit in &mut print_name {
        if *code_unit == '/' as u16 {
            *code_unit = '\\' as u16;
        }
    }

    let mut substitute = Vec::from(NON_INTERPRETED_PATH_PREFIX);
    substitute.extend(print_name.iter().copied());
    let substitute_len = substitute
        .len()
        .checked_mul(WCHAR_SIZE)
        .ok_or(io::Error::from_raw_os_error(ERROR_INVALID_PARAMETER as i32))?;
    let substitute_len = u16::try_from(substitute_len)
        .map_err(|_| io::Error::from_raw_os_error(ERROR_INVALID_PARAMETER as i32))?;
    let print_name_len = print_name
        .len()
        .checked_mul(WCHAR_SIZE)
        .ok_or(io::Error::from_raw_os_error(ERROR_INVALID_PARAMETER as i32))?;
    let print_name_len = u16::try_from(print_name_len)
        .map_err(|_| io::Error::from_raw_os_error(ERROR_INVALID_PARAMETER as i32))?;
    let print_name_offset = substitute_len
        .checked_add(UNICODE_NULL_SIZE as u16)
        .ok_or(io::Error::from_raw_os_error(ERROR_INVALID_PARAMETER as i32))?;
    let reparse_data_len = (MOUNT_POINT_HEADER_LEN as u16)
        .checked_add(print_name_offset)
        .and_then(|len| len.checked_add(print_name_len))
        .and_then(|len| len.checked_add(UNICODE_NULL_SIZE as u16))
        .ok_or(io::Error::from_raw_os_error(ERROR_INVALID_PARAMETER as i32))?;

    let mut data = Vec::with_capacity(8 + usize::from(reparse_data_len));
    data.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    data.extend_from_slice(&reparse_data_len.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&substitute_len.to_le_bytes());
    data.extend_from_slice(&print_name_offset.to_le_bytes());
    data.extend_from_slice(&print_name_len.to_le_bytes());
    for code_unit in substitute {
        data.extend_from_slice(&code_unit.to_le_bytes());
    }
    data.extend_from_slice(&0u16.to_le_bytes());
    for code_unit in print_name {
        data.extend_from_slice(&code_unit.to_le_bytes());
    }
    data.extend_from_slice(&0u16.to_le_bytes());
    Ok(data)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn renaming_a_junction_preserves_its_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let old_link = dir.path().join("old-link");
        let new_link = dir.path().join("new-link");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("data"), b"unchanged").unwrap();
        create_junction(target.as_os_str(), old_link.as_os_str()).unwrap();
        crate::filesystem::os::rename(
            &old_link,
            &new_link,
            crate::filesystem::os::RenameOptions {
                replace: false,
                cross_device: crate::filesystem::os::CrossDevice::AllowCopy,
            },
        )
        .unwrap();
        assert!(target.is_dir());
        assert!(!old_link.exists());
        assert_eq!(std::fs::read(new_link.join("data")).unwrap(), b"unchanged");
    }

    #[test]
    fn symlink_target_type_is_relative_to_the_link_parent() {
        let root = tempfile::tempdir().unwrap();
        let links = root.path().join("links");
        std::fs::create_dir(&links).unwrap();
        // A unique name makes the old process-cwd probe deterministically fail.
        let directory = tempfile::tempdir_in(root.path()).unwrap();
        let target = std::path::Path::new("..").join(directory.path().file_name().unwrap());
        let link = links.join("alias");

        assert!(symlink_target_is_directory(
            target.as_os_str(),
            link.as_os_str()
        ));
        assert!(symlink_target_is_directory(
            directory.path().as_os_str(),
            link.as_os_str()
        ));
        std::fs::write(root.path().join("file"), []).unwrap();
        assert!(!symlink_target_is_directory(
            std::ffi::OsStr::new(r"..\file"),
            link.as_os_str()
        ));
    }

    #[test]
    fn junction_reparse_buffer_encodes_substitute_and_print_names() {
        let data = junction_reparse_buffer(OsStr::new("C:/target")).unwrap();
        let reparse_data_len = u16::from_le_bytes([data[4], data[5]]) as usize;
        let substitute_len = u16::from_le_bytes([data[10], data[11]]) as usize;
        let print_name_offset = u16::from_le_bytes([data[12], data[13]]) as usize;
        let print_name_len = u16::from_le_bytes([data[14], data[15]]) as usize;

        assert_eq!(data.len(), 8 + reparse_data_len);
        assert_eq!(print_name_offset, substitute_len + 2);

        let path_buffer = &data[16..];
        let substitute = path_buffer[..substitute_len]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        let print_name = path_buffer[print_name_offset..print_name_offset + print_name_len]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();

        assert_eq!(String::from_utf16(&substitute).unwrap(), r"\??\C:\target");
        assert_eq!(String::from_utf16(&print_name).unwrap(), r"C:\target");
    }
}
