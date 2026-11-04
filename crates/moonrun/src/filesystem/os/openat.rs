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

//! Handle-relative opening from async's fs.c at 441364394f310ad002f42f589b0aca3f5def5835.
//! Paths with a parent stay relative to that handle throughout the operation;
//! resolving the parent's pathname first would reintroduce rename races.

use std::ffi::OsStr;
use std::fs::File;

use super::{BlockingIo, RawFile};
use std::io;

pub(in crate::filesystem) enum Access {
    Read,
    Write,
    ReadWrite,
    ListDirectory,
    /// Acquire a directory for relative operations without enumeration access.
    TraverseDirectory,
    #[cfg_attr(unix, allow(dead_code))] // Used for Windows metadata handles.
    Attributes,
    #[cfg_attr(unix, allow(dead_code))]
    Delete,
}

/// An object-type requirement enforced by the native open, separate from the
/// access granted to the resulting handle.
pub(in crate::filesystem) enum TypeConstraint {
    Any,
    Directory,
    // Unix file-only deletion uses unlinkat directly, without opening a handle.
    #[cfg(windows)]
    NonDirectory,
}

pub(in crate::filesystem) enum CreateMode {
    Open,
    Truncate,
    Create,
    CreateOrTruncate,
    Exclusive,
}

pub(in crate::filesystem) enum Sync {
    None,
    Data,
    All,
}

pub(in crate::filesystem) struct OpenOptions {
    pub access: Access,
    pub type_constraint: TypeConstraint,
    pub follow_symlink: bool,
    pub is_async: bool,
    pub create: CreateMode,
    pub append: bool,
    pub sync: Sync,
    pub permission: u32,
}

#[cfg(unix)]
pub(in crate::filesystem) fn open(
    parent: Option<RawFile>,
    path: &OsStr,
    options: &OpenOptions,
    blocking: &impl BlockingIo,
) -> io::Result<File> {
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStrExt;

    let path = std::ffi::CString::new(path.as_bytes())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let access = match options.access {
        Access::Read | Access::ListDirectory | Access::Attributes | Access::Delete => {
            libc::O_RDONLY
        }
        #[cfg(target_os = "linux")]
        Access::TraverseDirectory => libc::O_PATH,
        #[cfg(target_os = "macos")]
        Access::TraverseDirectory => libc::O_EVTONLY,
        Access::Write => libc::O_WRONLY,
        Access::ReadWrite => libc::O_RDWR,
    };
    let create = match options.create {
        CreateMode::Open => 0,
        CreateMode::Truncate => libc::O_TRUNC,
        CreateMode::Create => libc::O_CREAT,
        CreateMode::CreateOrTruncate => libc::O_CREAT | libc::O_TRUNC,
        CreateMode::Exclusive => libc::O_CREAT | libc::O_EXCL,
    };
    let sync = match options.sync {
        Sync::None => 0,
        Sync::Data => libc::O_DSYNC,
        Sync::All => libc::O_SYNC,
    };
    let mut flags = access | create | sync | libc::O_CLOEXEC;
    if options.append {
        flags |= libc::O_APPEND;
    }
    // Reject non-directories before openat can wait for a FIFO writer.
    if matches!(options.type_constraint, TypeConstraint::Directory) {
        flags |= libc::O_DIRECTORY;
    }
    if !options.follow_symlink {
        #[cfg(target_os = "linux")]
        {
            flags |= libc::O_NOFOLLOW;
        }
        #[cfg(target_os = "macos")]
        {
            flags |= libc::O_SYMLINK;
        }
    }
    // is_async selects overlapped directory handles on Windows only.
    let _ = options.is_async;
    let parent = parent.filter(|fd| *fd >= 0).unwrap_or(libc::AT_FDCWD);
    blocking.run(|| {
        let fd = unsafe {
            libc::openat(
                parent,
                path.as_ptr(),
                flags,
                options.permission as libc::c_uint,
            )
        };
        #[cfg(target_os = "linux")]
        let fd = if fd < 0
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ELOOP)
            && !options.follow_symlink
        {
            unsafe {
                libc::openat(
                    parent,
                    path.as_ptr(),
                    flags | libc::O_PATH,
                    options.permission as libc::c_uint,
                )
            }
        } else {
            fd
        };
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    })
}

#[cfg(windows)]
pub(in crate::filesystem) use windows::open;

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::{ffi::OsStrExt, io::FromRawHandle};
    use std::sync::OnceLock;
    use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
    use windows_sys::Wdk::Storage::FileSystem::{
        FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_IF,
        FILE_OPEN_REPARSE_POINT, FILE_OVERWRITE, FILE_OVERWRITE_IF, FILE_SYNCHRONOUS_IO_NONALERT,
        FILE_WRITE_THROUGH, NtCreateFile,
    };
    use windows_sys::Win32::Foundation::{
        ERROR_FILENAME_EXCED_RANGE, ERROR_PIPE_BUSY, ERROR_PROC_NOT_FOUND, GENERIC_READ,
        GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, NTSTATUS, OBJ_CASE_INSENSITIVE,
        RtlNtStatusToDosError, UNICODE_STRING,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, SYNCHRONIZE,
    };
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows_sys::Win32::System::Pipes::{NMPWAIT_WAIT_FOREVER, WaitNamedPipeW};
    use windows_sys::Win32::System::WindowsProgramming::RtlFreeUnicodeString;

    // These two NT routines and RTL_RELATIVE_NAME_U are absent from windows-sys.
    // The relative-name reference pins the process cwd until NtCreateFile ends.
    #[repr(C)]
    #[derive(Default)]
    struct RelativeName {
        name: UNICODE_STRING,
        directory: HANDLE,
        cwd_ref: *mut core::ffi::c_void,
    }
    type ConvertPath = unsafe extern "system" fn(
        *const u16,
        *mut UNICODE_STRING,
        *mut *mut u16,
        *mut RelativeName,
    ) -> NTSTATUS;
    type ReleaseName = unsafe extern "system" fn(*mut RelativeName);

    fn relative_path_functions() -> io::Result<(ConvertPath, ReleaseName)> {
        static FUNCTIONS: OnceLock<Option<(ConvertPath, ReleaseName)>> = OnceLock::new();
        FUNCTIONS
            .get_or_init(|| unsafe {
                let module = GetModuleHandleW(windows_sys::core::w!("ntdll.dll"));
                if module.is_null() {
                    return None;
                }
                let convert = GetProcAddress(
                    module,
                    c"RtlDosPathNameToRelativeNtPathName_U_WithStatus"
                        .as_ptr()
                        .cast(),
                )?;
                let release = GetProcAddress(module, c"RtlReleaseRelativeName".as_ptr().cast())?;
                // The export names above have exactly these NT system signatures.
                Some((
                    std::mem::transmute::<unsafe extern "system" fn() -> isize, ConvertPath>(
                        convert,
                    ),
                    std::mem::transmute::<unsafe extern "system" fn() -> isize, ReleaseName>(
                        release,
                    ),
                ))
            })
            .ok_or(io::Error::from_raw_os_error(ERROR_PROC_NOT_FOUND as i32))
    }

    struct DosPath {
        name: UNICODE_STRING,
        relative: RelativeName,
        release: ReleaseName,
    }

    impl Drop for DosPath {
        fn drop(&mut self) {
            unsafe {
                (self.release)(&mut self.relative);
                RtlFreeUnicodeString(&mut self.name);
            }
        }
    }

    pub(in crate::filesystem) fn open(
        parent: Option<RawFile>,
        path: &OsStr,
        options: &OpenOptions,
        blocking: &impl BlockingIo,
    ) -> io::Result<File> {
        let mut filename: Vec<u16> = path.encode_wide().collect();
        if filename.contains(&0) {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        filename.push(0);
        let mut access = match options.access {
            Access::Read => GENERIC_READ,
            Access::Write => GENERIC_WRITE | FILE_READ_ATTRIBUTES,
            Access::ReadWrite => GENERIC_READ | GENERIC_WRITE,
            Access::ListDirectory => FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            Access::TraverseDirectory => FILE_READ_ATTRIBUTES | FILE_TRAVERSE,
            Access::Attributes => FILE_READ_ATTRIBUTES,
            Access::Delete => DELETE,
        };
        let create = match options.create {
            CreateMode::Open => FILE_OPEN,
            CreateMode::Truncate => FILE_OVERWRITE,
            CreateMode::Create => FILE_OPEN_IF,
            CreateMode::CreateOrTruncate => FILE_OVERWRITE_IF,
            CreateMode::Exclusive => FILE_CREATE,
        };
        let mut flags = match options.sync {
            Sync::None => 0,
            Sync::Data | Sync::All => FILE_WRITE_THROUGH,
        };
        // Append changes writing behavior without granting write access to
        // read-only or metadata handles.
        if options.append && matches!(options.access, Access::Write | Access::ReadWrite) {
            access = (access & !GENERIC_WRITE) | FILE_APPEND_DATA;
        }
        if !options.is_async {
            access |= SYNCHRONIZE;
            flags |= FILE_SYNCHRONOUS_IO_NONALERT;
        }
        if !options.follow_symlink {
            flags |= FILE_OPEN_REPARSE_POINT;
        }
        match options.type_constraint {
            TypeConstraint::Any => {}
            TypeConstraint::Directory => flags |= FILE_DIRECTORY_FILE,
            TypeConstraint::NonDirectory => flags |= FILE_NON_DIRECTORY_FILE,
        }

        let _ = options.permission; // Unix permission bits have no Windows equivalent.
        let parent = parent.filter(|handle| *handle != INVALID_HANDLE_VALUE);
        let mut relative_name = UNICODE_STRING::default();
        let mut dos_path;
        let (name, root) = if let Some(parent) = parent {
            let len = u16::try_from((filename.len() - 1) * 2)
                .map_err(|_| io::Error::from_raw_os_error(ERROR_FILENAME_EXCED_RANGE as i32))?;
            relative_name.Length = len;
            relative_name.MaximumLength = len;
            relative_name.Buffer = filename.as_mut_ptr();
            (&mut relative_name, parent)
        } else {
            let (convert, release) = relative_path_functions()?;
            let mut name = UNICODE_STRING::default();
            let mut relative = RelativeName::default();
            let status = unsafe {
                convert(
                    filename.as_ptr(),
                    &mut name,
                    std::ptr::null_mut(),
                    &mut relative,
                )
            };
            if status < 0 {
                return Err(io::Error::from_raw_os_error(
                    unsafe { RtlNtStatusToDosError(status) } as i32,
                ));
            }
            dos_path = DosPath {
                name,
                relative,
                release,
            };
            if dos_path.relative.name.Length != 0 {
                (&mut dos_path.relative.name, dos_path.relative.directory)
            } else {
                (&mut dos_path.name, std::ptr::null_mut())
            }
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: root,
            ObjectName: name,
            Attributes: OBJ_CASE_INSENSITIVE,
            ..Default::default()
        };
        loop {
            let mut handle = INVALID_HANDLE_VALUE;
            let mut io = IO_STATUS_BLOCK::default();
            let status = blocking.run(|| {
                Ok(unsafe {
                    NtCreateFile(
                        &mut handle,
                        access,
                        &attributes,
                        &mut io,
                        std::ptr::null(),
                        FILE_ATTRIBUTE_NORMAL,
                        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                        create,
                        flags,
                        std::ptr::null(),
                        0,
                    )
                })
            })?;
            if status >= 0 {
                return Ok(unsafe { File::from_raw_handle(handle) });
            }
            let error = unsafe { RtlNtStatusToDosError(status) };
            if error != ERROR_PIPE_BUSY || parent.is_some() {
                return Err(io::Error::from_raw_os_error(error as i32));
            }
            blocking.run(|| {
                if unsafe { WaitNamedPipeW(filename.as_ptr(), NMPWAIT_WAIT_FOREVER) } == 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            })?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const READ: OpenOptions = OpenOptions {
        access: Access::Read,
        type_constraint: TypeConstraint::Any,
        follow_symlink: true,
        is_async: false,
        create: CreateMode::Open,
        append: false,
        sync: Sync::None,
        permission: 0o600,
    };

    fn open(parent: Option<RawFile>, path: &OsStr, options: &OpenOptions) -> io::Result<File> {
        super::open(parent, path, options, &super::super::DirectIo)
    }

    #[test]
    fn directory_only_open_rejects_regular_files() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("file");
        std::fs::write(&path, b"keep").unwrap();
        let parent = open(None, root.path().as_os_str(), &READ).unwrap();
        let options = OpenOptions {
            access: Access::ListDirectory,
            type_constraint: TypeConstraint::Directory,
            is_async: true,
            ..READ
        };
        #[cfg(unix)]
        let expected = io::Error::from_raw_os_error(libc::ENOTDIR);
        #[cfg(windows)]
        let expected =
            io::Error::from_raw_os_error(windows_sys::Win32::Foundation::ERROR_DIRECTORY as i32);
        for (parent, path) in [
            (None, path.as_os_str()),
            (Some(super::super::raw_file(&parent)), OsStr::new("file")),
        ] {
            assert_eq!(
                open(parent, path, &options).unwrap_err().raw_os_error(),
                expected.raw_os_error()
            );
            assert!(
                open(parent, path, &READ)
                    .unwrap()
                    .metadata()
                    .unwrap()
                    .is_file()
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn directory_only_open_rejects_fifos() {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::OpenOptionsExt;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("fifo");
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        // Keep a writer present so a missing O_DIRECTORY fails the assertion
        // rather than hanging this test indefinitely inside openat.
        let _keeper = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .unwrap();
        let parent = open(None, root.path().as_os_str(), &READ).unwrap();
        let options = OpenOptions {
            access: Access::ListDirectory,
            type_constraint: TypeConstraint::Directory,
            is_async: true,
            ..READ
        };
        for (parent, path) in [
            (None, path.as_os_str()),
            (Some(super::super::raw_file(&parent)), OsStr::new("fifo")),
        ] {
            assert_eq!(
                open(parent, path, &options).unwrap_err().raw_os_error(),
                Some(libc::ENOTDIR),
            );
            // Ordinary read opens remain usable by already-compiled guests.
            assert!(open(parent, path, &READ).is_ok());
        }
    }

    #[test]
    fn relative_open_keeps_the_opened_directory_after_rename() {
        use std::io::Read;
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("original");
        let moved = root.path().join("moved");
        std::fs::create_dir(&original).unwrap();
        std::fs::write(original.join("file"), b"original").unwrap();
        let parent = open(None, original.as_os_str(), &READ).unwrap();
        std::fs::rename(&original, &moved).unwrap();
        std::fs::create_dir(&original).unwrap();
        std::fs::write(original.join("file"), b"replacement").unwrap();

        let mut content = String::new();
        open(
            Some(super::super::raw_file(&parent)),
            OsStr::new("file"),
            &READ,
        )
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();
        assert_eq!(content, "original");
        assert_eq!(
            std::fs::read(original.join("file")).unwrap(),
            b"replacement"
        );
    }

    #[cfg(unix)]
    #[test]
    fn nofollow_opens_the_symlink_itself() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("target")).unwrap();
        std::os::unix::fs::symlink("target", root.path().join("link")).unwrap();
        let parent = open(None, root.path().as_os_str(), &READ).unwrap();
        let link = open(
            Some(super::super::raw_file(&parent)),
            OsStr::new("link"),
            &OpenOptions {
                follow_symlink: false,
                ..READ
            },
        )
        .unwrap();
        assert!(link.metadata().unwrap().file_type().is_symlink());
        let target = open(
            Some(super::super::raw_file(&parent)),
            OsStr::new("link"),
            &READ,
        )
        .unwrap();
        assert!(target.metadata().unwrap().is_dir());
    }

    #[cfg(windows)]
    #[test]
    fn write_only_open_still_allows_metadata_queries() {
        use std::io::Write;
        let root = tempfile::tempdir().unwrap();
        let mut file = open(
            None,
            root.path().join("file").as_os_str(),
            &OpenOptions {
                access: Access::Write,
                create: CreateMode::Exclusive,
                ..READ
            },
        )
        .unwrap();
        file.write_all(b"metadata").unwrap();
        assert_eq!(file.metadata().unwrap().len(), 8);
    }
}
