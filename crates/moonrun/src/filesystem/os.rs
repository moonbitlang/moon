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

//! Operating-system filesystem mechanisms, independent of runtime policy and ABIs.

use std::{io, path::Path};

/// Cross-device copying is a permitted Windows fallback, never a guarantee.
/// Unix rename does not copy between filesystems.
#[derive(Clone, Copy)]
pub(super) enum CrossDevice {
    Reject,
    AllowCopy,
}

pub(super) struct RenameOptions {
    pub(super) replace: bool,
    pub(super) cross_device: CrossDevice,
}

#[cfg(unix)]
pub(super) fn rename(old_path: &Path, new_path: &Path, options: RenameOptions) -> io::Result<()> {
    let RenameOptions {
        replace,
        cross_device: _cross_device,
    } = options;
    // Ordinary replacement needs no extended rename syscall. Keep the standard
    // library's native route, including on kernels/profiles without renameat2.
    if replace {
        return std::fs::rename(old_path, new_path);
    }
    let old_path = path_to_cstring(old_path)?;
    let new_path = path_to_cstring(new_path)?;
    #[cfg(target_os = "linux")]
    let ret = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            old_path.as_ptr(),
            libc::AT_FDCWD,
            new_path.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(target_os = "macos")]
    let ret = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            old_path.as_ptr(),
            libc::AT_FDCWD,
            new_path.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub(super) fn rename(old_path: &Path, new_path: &Path, options: RenameOptions) -> io::Result<()> {
    let RenameOptions {
        replace,
        cross_device,
    } = options;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{
        ERROR_INVALID_ADDRESS, ERROR_INVALID_PARAMETER, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, DELETE, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FileRenameInfoEx, MOVEFILE_COPY_ALLOWED, MOVEFILE_REPLACE_EXISTING,
        MoveFileExW, OPEN_EXISTING, SetFileInformationByHandle,
    };

    // std already implements replacement without COPY_ALLOWED, including its
    // long-path conversion. Reuse that contract instead of duplicating it.
    if replace && matches!(cross_device, CrossDevice::Reject) {
        return std::fs::rename(old_path, new_path);
    }

    let old_path = path_to_wide(old_path);
    let new_path = path_to_wide(new_path);
    let handle: HANDLE = unsafe {
        CreateFileW(
            old_path.as_ptr(),
            DELETE,
            FILE_SHARE_DELETE | FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let handle = unsafe { OwnedHandle::from_raw_handle(handle) };

    let filename_len = new_path
        .len()
        .checked_sub(1)
        .ok_or_else(|| io::Error::from_raw_os_error(ERROR_INVALID_PARAMETER as i32))?;
    let buffer_size = std::mem::size_of::<FILE_RENAME_INFO>()
        .checked_add(filename_len * std::mem::size_of::<u16>())
        .ok_or_else(|| io::Error::from_raw_os_error(ERROR_INVALID_ADDRESS as i32))?;
    // The variable-length Win32 structure contains pointer-aligned fields.
    let mut buffer = vec![0usize; buffer_size.div_ceil(std::mem::size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).Anonymous.Flags = if replace { 3 } else { 0 };
        (*info).RootDirectory = std::ptr::null_mut();
        (*info).FileNameLength = u32::try_from(filename_len * std::mem::size_of::<u16>())
            .map_err(|_| io::Error::from_raw_os_error(ERROR_INVALID_ADDRESS as i32))?;
        std::ptr::copy_nonoverlapping(
            new_path.as_ptr(),
            (*info).FileName.as_mut_ptr(),
            filename_len,
        );
    }

    let ret = unsafe {
        SetFileInformationByHandle(
            handle.as_raw_handle(),
            FileRenameInfoEx,
            info.cast(),
            u32::try_from(buffer_size)
                .map_err(|_| io::Error::from_raw_os_error(ERROR_INVALID_ADDRESS as i32))?,
        )
    };
    if ret != 0 {
        return Ok(());
    }

    let error = io::Error::last_os_error();
    drop(handle);
    if error.raw_os_error() != Some(ERROR_INVALID_PARAMETER as i32) {
        return Err(error);
    }

    let flags = if matches!(cross_device, CrossDevice::AllowCopy) {
        MOVEFILE_COPY_ALLOWED
    } else {
        0
    } | if replace {
        MOVEFILE_REPLACE_EXISTING
    } else {
        0
    };
    if unsafe { MoveFileExW(old_path.as_ptr(), new_path.as_ptr(), flags) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn path_to_cstring(path: &Path) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))
}

#[cfg(windows)]
fn path_to_wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn no_copy_replacement_preserves_native_long_path_support() {
        use std::os::windows::ffi::OsStrExt;

        let directory = tempfile::tempdir().unwrap();
        let mut nested = dunce::simplified(directory.path()).to_path_buf();
        while nested.as_os_str().encode_wide().count() <= 280 {
            nested.push("long-directory-component");
        }
        std::fs::create_dir_all(&nested).unwrap();
        let source = nested.join("source");
        let target = nested.join("target");
        std::fs::write(&source, b"keep").unwrap();
        // The standard-library control accepts the same non-verbatim path.
        std::fs::rename(&source, &target).unwrap();
        rename(
            &target,
            &source,
            RenameOptions {
                replace: true,
                cross_device: CrossDevice::Reject,
            },
        )
        .unwrap();
        assert_eq!(std::fs::read(source).unwrap(), b"keep");
        assert!(!target.exists());
    }

    #[test]
    fn replacement_is_explicit_and_failed_rename_preserves_both_files() {
        for cross_device in [CrossDevice::Reject, CrossDevice::AllowCopy] {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("source");
            let target = directory.path().join("target");
            std::fs::write(&source, b"source").unwrap();
            std::fs::write(&target, b"target").unwrap();
            assert!(
                rename(
                    &source,
                    &target,
                    RenameOptions {
                        replace: false,
                        cross_device
                    }
                )
                .is_err()
            );
            assert_eq!(std::fs::read(&source).unwrap(), b"source");
            assert_eq!(std::fs::read(&target).unwrap(), b"target");
            rename(
                &source,
                &target,
                RenameOptions {
                    replace: true,
                    cross_device,
                },
            )
            .unwrap();
            assert!(!source.exists());
            assert_eq!(std::fs::read(&target).unwrap(), b"source");
        }
    }

    #[test]
    fn failed_rename_retains_the_os_error_code() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("missing");
        let target = directory.path().join("target");
        let expected = std::fs::rename(&source, &target).unwrap_err();
        assert!(expected.raw_os_error().is_some());
        for cross_device in [CrossDevice::Reject, CrossDevice::AllowCopy] {
            for replace in [false, true] {
                let error = rename(
                    &source,
                    &target,
                    RenameOptions {
                        replace,
                        cross_device,
                    },
                )
                .unwrap_err();
                assert_eq!(error.raw_os_error(), expected.raw_os_error());
            }
        }
    }
}
