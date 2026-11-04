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

//! Handle-relative removal from async's fs.c at e5c46afbcf03c5abd8132fdc1ce45bd8e9f525bc.

use std::ffi::OsStr;

use super::{BlockingIo, RawFile};
use std::io;

pub(in crate::filesystem) fn remove_at(
    parent: Option<RawFile>,
    path: &OsStr,
    blocking: &impl BlockingIo,
) -> io::Result<()> {
    #[cfg(unix)]
    {
        let error = match unlink_file_at(parent, path, blocking) {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        #[cfg(target_os = "macos")]
        let is_directory = error.raw_os_error() == Some(libc::EPERM);
        #[cfg(not(target_os = "macos"))]
        let is_directory = error.raw_os_error() == Some(libc::EISDIR);
        if !is_directory {
            return Err(error);
        }
        let result = remove_directory_at(parent, path, blocking);
        // Darwin's EPERM can also mean an immutable file, not a directory.
        #[cfg(target_os = "macos")]
        if result
            .as_ref()
            .is_err_and(|error| error.raw_os_error() == Some(libc::ENOTDIR))
        {
            return Err(error);
        }
        result
    }
    #[cfg(windows)]
    remove_windows(parent, path, RemovalKind::Any, blocking)
}

pub(in crate::filesystem) fn unlink_file_at(
    parent: Option<RawFile>,
    path: &OsStr,
    blocking: &impl BlockingIo,
) -> io::Result<()> {
    #[cfg(unix)]
    {
        let _ = blocking;
        unlink_unix(parent, path, 0)
    }
    #[cfg(windows)]
    remove_windows(parent, path, RemovalKind::File, blocking)
}

pub(in crate::filesystem) fn remove_directory_at(
    parent: Option<RawFile>,
    path: &OsStr,
    blocking: &impl BlockingIo,
) -> io::Result<()> {
    #[cfg(unix)]
    {
        let _ = blocking;
        unlink_unix(parent, path, libc::AT_REMOVEDIR)
    }
    #[cfg(windows)]
    remove_windows(parent, path, RemovalKind::Directory, blocking)
}

#[cfg(unix)]
fn unlink_unix(parent: Option<RawFile>, path: &OsStr, flags: i32) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_bytes())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let parent = parent.filter(|fd| *fd >= 0).unwrap_or(libc::AT_FDCWD);
    if unsafe { libc::unlinkat(parent, path.as_ptr(), flags) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
enum RemovalKind {
    Any,
    File,
    Directory,
}

#[cfg(windows)]
fn remove_windows(
    parent: Option<RawFile>,
    path: &OsStr,
    kind: RemovalKind,
    blocking: &impl BlockingIo,
) -> io::Result<()> {
    use super::openat::{self, Access, CreateMode, OpenOptions, Sync, TypeConstraint};
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{
        ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
        FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX,
        FileDispositionInfo, FileDispositionInfoEx, SetFileInformationByHandle,
    };
    let file = openat::open(
        parent,
        path,
        &OpenOptions {
            // Enforce entry kind in the same native open that acquires the
            // deletion handle, without requiring metadata-read permission.
            access: Access::Delete,
            type_constraint: match kind {
                RemovalKind::Any => TypeConstraint::Any,
                RemovalKind::File => TypeConstraint::NonDirectory,
                RemovalKind::Directory => TypeConstraint::Directory,
            },
            follow_symlink: false,
            is_async: false,
            create: CreateMode::Open,
            append: false,
            sync: Sync::None,
            permission: 0,
        },
        blocking,
    )?;
    let info = FILE_DISPOSITION_INFO_EX {
        // std::fs::remove_file also permits deleting read-only files on
        // Windows. Keep that WASI behavior without changing async remove.
        Flags: FILE_DISPOSITION_FLAG_DELETE
            | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
            | if matches!(kind, RemovalKind::File) {
                FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE
            } else {
                0
            },
    };
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfoEx,
            std::ptr::from_ref(&info).cast(),
            std::mem::size_of_val(&info) as u32,
        )
    } != 0
    {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if !matches!(
        error.raw_os_error().unwrap_or_default() as u32,
        ERROR_INVALID_FUNCTION | ERROR_INVALID_PARAMETER | ERROR_NOT_SUPPORTED
    ) {
        return Err(error);
    }
    // Use native's older deletion mode when the OS or filesystem does not
    // support extended disposition. Other errors must not trigger a retry.
    let info = FILE_DISPOSITION_INFO { DeleteFile: true };
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            std::ptr::from_ref(&info).cast(),
            std::mem::size_of_val(&info) as u32,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use std::fs::File;

    #[test]
    fn relative_removal_keeps_the_opened_directory_after_rename() {
        use super::super::openat::{self, Access, CreateMode, OpenOptions, Sync, TypeConstraint};
        use super::super::{DirectIo, raw_file};

        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("original");
        let moved = root.path().join("moved");
        std::fs::create_dir(&original).unwrap();
        std::fs::write(original.join("file"), b"original").unwrap();
        let parent = openat::open(
            None,
            original.as_os_str(),
            &OpenOptions {
                access: Access::ListDirectory,
                type_constraint: TypeConstraint::Directory,
                follow_symlink: true,
                is_async: false,
                create: CreateMode::Open,
                append: false,
                sync: Sync::None,
                permission: 0,
            },
            &DirectIo,
        )
        .unwrap();
        std::fs::rename(&original, &moved).unwrap();
        std::fs::create_dir(&original).unwrap();
        std::fs::write(original.join("file"), b"replacement").unwrap();

        remove_at(Some(raw_file(&parent)), OsStr::new("file"), &DirectIo).unwrap();
        assert!(!moved.join("file").exists());
        assert_eq!(
            std::fs::read(original.join("file")).unwrap(),
            b"replacement"
        );
        std::fs::create_dir(moved.join("empty")).unwrap();
        remove_at(Some(raw_file(&parent)), OsStr::new("empty"), &DirectIo).unwrap();
        assert!(!moved.join("empty").exists());
        std::fs::create_dir(moved.join("nonempty")).unwrap();
        std::fs::write(moved.join("nonempty/file"), []).unwrap();
        assert!(remove_at(Some(raw_file(&parent)), OsStr::new("nonempty"), &DirectIo).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn removing_symlink_keeps_its_target() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("target")).unwrap();
        std::os::unix::fs::symlink("target", root.path().join("link")).unwrap();
        let parent = std::fs::File::open(root.path()).unwrap();
        remove_at(
            Some(super::super::raw_file(&parent)),
            OsStr::new("link"),
            &super::super::DirectIo,
        )
        .unwrap();
        assert!(root.path().join("target").is_dir());
        assert!(std::fs::symlink_metadata(root.path().join("link")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn file_only_removal_preserves_std_readonly_behavior() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("file");
        let control = root.path().join("control");
        for file in [&path, &control] {
            std::fs::write(file, b"keep").unwrap();
            let mut permissions = std::fs::metadata(file).unwrap().permissions();
            permissions.set_readonly(true);
            std::fs::set_permissions(file, permissions).unwrap();
        }
        let expected = std::fs::remove_file(&control);
        let actual = unlink_file_at(None, path.as_os_str(), &super::super::DirectIo);
        assert_eq!(actual.is_ok(), expected.is_ok(), "{actual:?}, {expected:?}");
    }

    #[cfg(windows)]
    #[test]
    fn typed_removal_preserves_std_symlink_behavior() {
        use std::os::windows::fs::{symlink_dir, symlink_file};
        for directory_link in [false, true] {
            for remove_directory in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let target = root.path().join("target");
                if directory_link {
                    std::fs::create_dir(&target).unwrap();
                } else {
                    std::fs::write(&target, b"keep").unwrap();
                }
                let path = root.path().join("link");
                let control = root.path().join("control");
                for link in [&path, &control] {
                    let result = if directory_link {
                        symlink_dir(&target, link)
                    } else {
                        symlink_file(&target, link)
                    };
                    if result.as_ref().is_err_and(|error| {
                        error.raw_os_error()
                            == Some(windows_sys::Win32::Foundation::ERROR_PRIVILEGE_NOT_HELD as i32)
                    }) {
                        eprintln!("Windows symlink privilege unavailable");
                        return;
                    }
                    result.unwrap();
                }
                let (actual, expected) = if remove_directory {
                    (
                        remove_directory_at(None, path.as_os_str(), &super::super::DirectIo),
                        std::fs::remove_dir(&control),
                    )
                } else {
                    (
                        unlink_file_at(None, path.as_os_str(), &super::super::DirectIo),
                        std::fs::remove_file(&control),
                    )
                };
                assert_eq!(
                    actual.is_ok(),
                    expected.is_ok(),
                    "directory_link={directory_link}, remove_directory={remove_directory}: {actual:?}, {expected:?}"
                );
                assert!(target.exists());
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn remove_immutable_file_preserves_eperm_instead_of_enotdir() {
        use std::os::fd::AsRawFd;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("immutable");
        let file = File::create(&path).unwrap();
        assert_eq!(
            unsafe { libc::fchflags(file.as_raw_fd(), libc::UF_IMMUTABLE) },
            0
        );
        // Restore flags before any assertion can unwind into TempDir cleanup.
        let result = remove_at(None, path.as_os_str(), &super::super::DirectIo);
        assert_eq!(unsafe { libc::fchflags(file.as_raw_fd(), 0) }, 0);
        assert_eq!(result.unwrap_err().raw_os_error(), Some(libc::EPERM));
    }
}
