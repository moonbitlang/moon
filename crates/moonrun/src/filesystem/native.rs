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

//! Native filesystem mechanisms; no Runtime state or policy decisions.

use crate::async_host::{AsyncHostError, AsyncHostResult};
use std::ffi::OsString;

/// Cross-device copying is a permitted fallback, never a guarantee. Unix
/// rename remains a single rename syscall. WASI disallows copy/delete fallback;
/// the native async ABI retains its existing Windows behavior explicitly.
#[derive(Clone, Copy)]
pub(super) enum CrossDevice {
    Reject,
    AllowCopy,
}

#[cfg(unix)]
pub(super) fn rename(
    old_path: OsString,
    new_path: OsString,
    replace: bool,
    _cross_device: CrossDevice,
) -> AsyncHostResult<()> {
    // Ordinary replacement needs no extended rename syscall. Keep the standard
    // library's native route, including on kernels/profiles without renameat2.
    if replace {
        return std::fs::rename(old_path, new_path).map_err(|error| {
            AsyncHostError::Native(
                error
                    .raw_os_error()
                    .unwrap_or_else(|| AsyncHostError::Inval.errno()),
            )
        });
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
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub(super) fn rename(
    old_path: OsString,
    new_path: OsString,
    replace: bool,
    cross_device: CrossDevice,
) -> AsyncHostResult<()> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{ERROR_INVALID_PARAMETER, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, DELETE, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FileRenameInfoEx, MOVEFILE_COPY_ALLOWED, MOVEFILE_REPLACE_EXISTING,
        MoveFileExW, OPEN_EXISTING, SetFileInformationByHandle,
    };

    // std already implements replacement without COPY_ALLOWED, including its
    // long-path conversion. Reuse that contract instead of duplicating it.
    if replace && matches!(cross_device, CrossDevice::Reject) {
        return std::fs::rename(old_path, new_path).map_err(|error| {
            AsyncHostError::Native(
                error
                    .raw_os_error()
                    .unwrap_or_else(|| AsyncHostError::Inval.errno()),
            )
        });
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
        return Err(last_native_error());
    }
    let handle = unsafe { OwnedHandle::from_raw_handle(handle) };

    let filename_len = new_path.len().checked_sub(1).ok_or(AsyncHostError::Inval)?;
    let buffer_size = std::mem::size_of::<FILE_RENAME_INFO>()
        .checked_add(filename_len * std::mem::size_of::<u16>())
        .ok_or(AsyncHostError::Fault)?;
    // The variable-length Win32 structure contains pointer-aligned fields.
    let mut buffer = vec![0usize; buffer_size.div_ceil(std::mem::size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).Anonymous.Flags = if replace { 3 } else { 0 };
        (*info).RootDirectory = std::ptr::null_mut();
        (*info).FileNameLength = u32::try_from(filename_len * std::mem::size_of::<u16>())
            .map_err(|_| AsyncHostError::Fault)?;
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
            u32::try_from(buffer_size).map_err(|_| AsyncHostError::Fault)?,
        )
    };
    if ret != 0 {
        return Ok(());
    }

    let error = std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_else(|| AsyncHostError::Inval.errno());
    drop(handle);
    if error != ERROR_INVALID_PARAMETER as i32 {
        return Err(AsyncHostError::Native(error));
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
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn path_to_cstring(path: OsString) -> AsyncHostResult<std::ffi::CString> {
    use std::os::unix::ffi::OsStringExt;
    std::ffi::CString::new(path.into_vec()).map_err(|_| AsyncHostError::Inval)
}

#[cfg(windows)]
fn path_to_wide(path: OsString) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.encode_wide().chain(std::iter::once(0)).collect()
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
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
            target.clone().into(),
            source.clone().into(),
            true,
            CrossDevice::Reject,
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
                    source.clone().into(),
                    target.clone().into(),
                    false,
                    cross_device
                )
                .is_err()
            );
            assert_eq!(std::fs::read(&source).unwrap(), b"source");
            assert_eq!(std::fs::read(&target).unwrap(), b"target");
            rename(
                source.clone().into(),
                target.clone().into(),
                true,
                cross_device,
            )
            .unwrap();
            assert!(!source.exists());
            assert_eq!(std::fs::read(&target).unwrap(), b"source");
        }
    }
}
