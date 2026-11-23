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

//! Directory-handle path lookup and native enumeration.
//! Enumeration follows async's fs.c at d53eadfc623d343edaf01f0b11dad28badcb1274.
//! Callers serialize access to the shared directory cursor. The return value is
//! the native enumeration result: entry count on macOS, buffer size on Windows,
//! and bytes written on Linux; zero means the directory is exhausted.

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

#[cfg(all(unix, target_os = "linux"))]
pub(in crate::filesystem) fn read(
    handle: RawFile,
    out: &mut [u8],
    restart: bool,
) -> io::Result<usize> {
    if restart && unsafe { libc::lseek(handle, 0, libc::SEEK_SET) } < 0 {
        return Err(io::Error::last_os_error());
    }

    let ret = unsafe { libc::syscall(libc::SYS_getdents64, handle, out.as_mut_ptr(), out.len()) };
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(ret as usize)
}

#[cfg(all(unix, target_os = "macos"))]
pub(in crate::filesystem) fn read(
    handle: RawFile,
    out: &mut [u8],
    restart: bool,
) -> io::Result<usize> {
    if restart && unsafe { libc::lseek(handle, 0, libc::SEEK_SET) } < 0 {
        return Err(io::Error::last_os_error());
    }

    let mut attr_spec = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_RETURNED_ATTRS
            | libc::ATTR_CMN_NAME
            | libc::ATTR_CMN_OBJTYPE
            | libc::ATTR_CMN_FILEID,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let ret = unsafe {
        libc::getattrlistbulk(
            handle,
            (&mut attr_spec as *mut libc::attrlist).cast(),
            out.as_mut_ptr().cast(),
            out.len(),
            0,
        )
    };
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(ret as usize)
}

#[cfg(windows)]
pub(in crate::filesystem) fn read(
    handle: RawFile,
    out: &mut [u8],
    restart: bool,
) -> io::Result<usize> {
    use windows_sys::Win32::Foundation::{ERROR_INVALID_ADDRESS, ERROR_NO_MORE_FILES};
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo, GetFileInformationByHandleEx,
    };

    let info_class = if restart {
        FileIdBothDirectoryRestartInfo
    } else {
        FileIdBothDirectoryInfo
    };
    let ok = unsafe {
        GetFileInformationByHandleEx(
            handle,
            info_class,
            out.as_mut_ptr().cast(),
            u32::try_from(out.len())
                .map_err(|_| io::Error::from_raw_os_error(ERROR_INVALID_ADDRESS as i32))?,
        )
    };
    if ok == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
            return Ok(0);
        }
        return Err(error);
    }

    Ok(out.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumeration_can_restart_after_exhaustion() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("entry"), b"data").unwrap();
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options
                .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS);
        }
        let directory = options.open(dir.path()).unwrap();
        let handle = crate::filesystem::os::raw_file(&directory);
        let mut buffer = vec![0; 4096];

        let first = read(handle, &mut buffer, false).unwrap();
        assert!(first > 0);
        assert_eq!(read(handle, &mut buffer, false).unwrap(), 0);
        assert_eq!(read(handle, &mut buffer, true).unwrap(), first);
    }
}
