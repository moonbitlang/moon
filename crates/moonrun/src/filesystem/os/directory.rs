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

//! Recover a directory handle's current location for path-based authorization.
//! This is not used to reopen the directory for relative file operations.

use super::RawFile;
use std::{io, path::PathBuf};

#[cfg(unix)]
pub(in crate::filesystem) fn path(handle: RawFile) -> io::Result<PathBuf> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(handle, stat.as_mut_ptr()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let stat = unsafe { stat.assume_init() };

    // An unlinked directory no longer has a path that policy can authorize. In
    // particular, never authorize a replacement installed at its former path.
    if stat.st_nlink == 0 {
        return Err(io::ErrorKind::NotFound.into());
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/self/fd/{}", handle))
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
        let mut path = [0u8; libc::PATH_MAX as usize];
        if unsafe { libc::fcntl(handle, libc::F_GETPATH, path.as_mut_ptr()) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let path = std::ffi::CStr::from_bytes_until_nul(&path)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        let path = std::path::Path::new(std::ffi::OsStr::from_bytes(path.to_bytes()));
        // F_GETPATH can retain a removed directory's name even when its link
        // count is nonzero. Never let a replacement at that name supply the
        // policy base for relative operations on the original handle.
        let named = std::fs::metadata(path)?;
        if named.dev() != stat.st_dev as u64 || named.ino() != stat.st_ino {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(path.to_owned())
    }
}

#[cfg(windows)]
pub(in crate::filesystem) fn path(handle: RawFile) -> io::Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_NAME_NORMALIZED, FILE_STANDARD_INFO, FileStandardInfo, GetFileInformationByHandleEx,
        GetFinalPathNameByHandleW, VOLUME_NAME_DOS,
    };
    let mut info = FILE_STANDARD_INFO::default();
    if unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileStandardInfo,
            std::ptr::from_mut(&mut info).cast(),
            std::mem::size_of_val(&info) as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if info.DeletePending || info.NumberOfLinks == 0 {
        return Err(io::ErrorKind::NotFound.into());
    }
    let mut buffer = vec![0u16; 1024];
    loop {
        let len = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            )
        };
        if len == 0 {
            return Err(io::Error::last_os_error());
        }
        if (len as usize) < buffer.len() {
            return Ok(std::ffi::OsString::from_wide(&buffer[..len as usize]).into());
        }
        buffer.resize(len as usize + 1, 0);
    }
}
