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

//! Native file metadata queries. Runtime resources and guest result layouts
//! are interpreted by callers; these operations return native values and errors.

use std::ffi::OsStr;
use std::io;

use super::RawFile;

#[cfg(unix)]
pub(in crate::filesystem) type FileInfo = libc::stat;
#[cfg(windows)]
pub(in crate::filesystem) type FileInfo = windows_sys::Win32::Storage::FileSystem::FILE_BASIC_INFO;

#[cfg(unix)]
pub(in crate::filesystem) fn file_info(file: RawFile) -> io::Result<FileInfo> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(file, stat.as_mut_ptr()) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { stat.assume_init() })
    }
}

#[cfg(unix)]
pub(in crate::filesystem) fn file_info_by_path(
    path: &OsStr,
    follow_symlink: bool,
) -> io::Result<FileInfo> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_bytes())?;
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    let ret = if follow_symlink {
        unsafe { libc::stat(path.as_ptr(), stat.as_mut_ptr()) }
    } else {
        unsafe { libc::lstat(path.as_ptr(), stat.as_mut_ptr()) }
    };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { stat.assume_init() })
    }
}

#[cfg(unix)]
pub(in crate::filesystem) fn file_info_at(
    parent: Option<RawFile>,
    path: &OsStr,
    follow_symlink: bool,
) -> io::Result<FileInfo> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_bytes())?;
    let flags = if follow_symlink {
        0
    } else {
        libc::AT_SYMLINK_NOFOLLOW
    };
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    let ret = unsafe {
        libc::fstatat(
            parent.unwrap_or(libc::AT_FDCWD),
            path.as_ptr(),
            stat.as_mut_ptr(),
            flags,
        )
    };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { stat.assume_init() })
    }
}

#[cfg(unix)]
#[allow(clippy::unnecessary_cast)]
pub(in crate::filesystem) fn file_size(file: RawFile) -> io::Result<i64> {
    Ok(file_info(file)?.st_size as i64)
}

#[cfg(windows)]
pub(in crate::filesystem) fn file_info(file: RawFile) -> io::Result<FileInfo> {
    use windows_sys::Win32::Storage::FileSystem::{FileBasicInfo, GetFileInformationByHandleEx};
    let mut info = std::mem::MaybeUninit::<FileInfo>::uninit();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file,
            FileBasicInfo,
            info.as_mut_ptr().cast(),
            std::mem::size_of::<FileInfo>() as u32,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { info.assume_init() })
    }
}

#[cfg(windows)]
pub(in crate::filesystem) fn file_info_by_path(
    path: &OsStr,
    follow_symlink: bool,
) -> io::Result<FileInfo> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    let mut flags = FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS;
    if !follow_symlink {
        flags |= FILE_FLAG_OPEN_REPARSE_POINT;
    }
    let path: Vec<u16> = path.encode_wide().chain(Some(0)).collect();
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            flags,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let file = unsafe { OwnedHandle::from_raw_handle(handle) };
    // Capture a failed query's error before closing its temporary handle.
    file_info(file.as_raw_handle())
}

#[cfg(windows)]
pub(in crate::filesystem) fn file_size(file: RawFile) -> io::Result<i64> {
    use windows_sys::Win32::Storage::FileSystem::GetFileSizeEx;
    let mut size = std::mem::MaybeUninit::<i64>::uninit();
    if unsafe { GetFileSizeEx(file, size.as_mut_ptr()) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { size.assume_init() })
    }
}

/// Observe the kernel file type and distinguish Winsock sockets from pipes.
#[cfg(windows)]
pub(in crate::filesystem) fn file_type(file: RawFile) -> io::Result<(u32, bool)> {
    use windows_sys::Win32::Foundation::{GetLastError, SetLastError};
    use windows_sys::Win32::Networking::WinSock::{SO_TYPE, SOCKET, SOL_SOCKET, getsockopt};
    use windows_sys::Win32::Storage::FileSystem::{FILE_TYPE_PIPE, FILE_TYPE_UNKNOWN, GetFileType};

    unsafe { SetLastError(0) };
    let file_type = unsafe { GetFileType(file) };
    let error = unsafe { GetLastError() };
    let is_socket = matches!(file_type, FILE_TYPE_PIPE | FILE_TYPE_UNKNOWN) && {
        let mut opt = 0i32;
        let mut opt_len = std::mem::size_of::<i32>() as i32;
        unsafe {
            getsockopt(
                file as SOCKET,
                SOL_SOCKET,
                SO_TYPE,
                (&mut opt as *mut i32).cast(),
                &mut opt_len,
            ) == 0
        }
    };
    if file_type == FILE_TYPE_UNKNOWN && error != 0 && !is_socket {
        // A Winsock SOCKET is not a kernel HANDLE. Probe it before propagating
        // GetFileType's error, and preserve that error across getsockopt.
        Err(io::Error::from_raw_os_error(error as i32))
    } else {
        Ok((file_type, is_socket))
    }
}
