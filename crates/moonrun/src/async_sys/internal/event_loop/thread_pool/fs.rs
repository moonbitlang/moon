// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Concrete filesystem job executors ported from
//! `moonbitlang/async/src/internal/event_loop/thread_pool.c`.

use std::ffi::OsString;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use crate::async_host::{AsyncHostError, AsyncHostResult, HostCBuffer};
use crate::async_sys::internal::fd_util;
use crate::async_sys::ported_fns;

use super::{FileTimeResult, OpenJobResource, OpenJobResult, RealpathJobResult, Resource};

type RawFile = fd_util::stub::RawFd;

#[cfg(unix)]
fn raw_file_handle(file: &Resource) -> AsyncHostResult<RawFile> {
    Ok(file.as_file()?.as_raw_fd())
}

#[cfg(windows)]
fn raw_file_handle(file: &Resource) -> AsyncHostResult<RawFile> {
    Ok(file.as_file()?.as_raw_handle())
}

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "open_job_worker"
    )]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_open_job(
        result: &mut Option<OpenJobResult>,
        filename: OsString,
        access: i32,
        create_mode: i32,
        append: bool,
        sync: i32,
        mode: i32,
    ) -> AsyncHostResult<i64> {
        let filename_for_policy = filename.clone();
        let OpenedFile {
            file,
            kind,
            dev_id,
            file_id,
        } = open_native_file(filename, access, create_mode, append, sync, mode)?;
        let policy_path = std::fs::canonicalize(filename_for_policy).ok();
        *result = Some(OpenJobResult {
            resource: OpenJobResource::Unpublished(Resource::new_with_policy_path(
                file,
                policy_path,
            )),
            kind,
            dev_id,
            file_id,
        });
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "read_job_worker"
    )]
    pub(super) fn run_read_job(
        file: &Resource,
        len: i32,
        position: i64,
        result: &mut Option<Vec<u8>>,
    ) -> AsyncHostResult<i64> {
        let mut buf = vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?];
        let n = read_from_native_file(raw_file_handle(file)?, &mut buf, position)?;
        buf.truncate(n);
        *result = Some(buf);
        Ok(n as i64)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "write_job_worker"
    )]
    pub(super) fn run_write_job(
        file: &Resource,
        data: &[u8],
        position: i64,
    ) -> AsyncHostResult<i64> {
        let n = write_to_native_file(raw_file_handle(file)?, data, position)?;
        Ok(n as i64)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "file_kind_by_path_job_worker"
    )]
    pub(super) fn run_file_kind_by_path_job(
        parent: Option<&Resource>,
        path: OsString,
        follow_symlink: bool,
    ) -> AsyncHostResult<i64> {
        file_kind_by_path(parent, path, follow_symlink).map(i64::from)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "file_size_job_worker"
    )]
    pub(super) fn run_file_size_job(
        file: &Resource,
        result: &mut i64,
    ) -> AsyncHostResult<i64> {
        *result = file_size(raw_file_handle(file)?)?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "file_time_job_worker"
    )]
    pub(super) fn run_file_time_job(
        file: &Resource,
        result: &mut Option<FileTimeResult>,
    ) -> AsyncHostResult<i64> {
        *result = Some(FileTimeResult::new(file_time(raw_file_handle(file)?)?));
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "file_time_by_path_job_worker"
    )]
    pub(super) fn run_file_time_by_path_job(
        path: OsString,
        follow_symlink: bool,
        result: &mut Option<FileTimeResult>,
    ) -> AsyncHostResult<i64> {
        *result = Some(FileTimeResult::new(file_time_by_path(
            path,
            follow_symlink,
        )?));
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "access_job_worker"
    )]
    pub(super) fn run_access_job(path: OsString, access: i32) -> AsyncHostResult<i64> {
        access_native_path(path, access)?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "chmod_job_worker"
    )]
    pub(super) fn run_chmod_job(path: OsString, mode: i32) -> AsyncHostResult<i64> {
        chmod_native_path(path, mode)?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "fsync_job_worker"
    )]
    pub(super) fn run_fsync_job(
        file: &Resource,
        only_data: bool,
    ) -> AsyncHostResult<i64> {
        sync_native_file(raw_file_handle(file)?, only_data)?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "flock_job_worker"
    )]
    pub(super) fn run_flock_job(
        file: &Resource,
        exclusive: bool,
    ) -> AsyncHostResult<i64> {
        lock_native_file(raw_file_handle(file)?, exclusive)?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "remove_job_worker"
    )]
    pub(super) fn run_remove_job(path: OsString) -> AsyncHostResult<i64> {
        remove_native_path(path)?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "rename_job_worker"
    )]
    pub(super) fn run_rename_job(
        old_path: OsString,
        new_path: OsString,
        replace: bool,
    ) -> AsyncHostResult<i64> {
        rename_native_path(old_path, new_path, replace)?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "symlink_job_worker"
    )]
    pub(super) fn run_symlink_job(
        target: OsString,
        path: OsString,
        force_symlink: bool,
    ) -> AsyncHostResult<i64> {
        symlink_native_path(target, path, force_symlink)?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "mkdir_job_worker"
    )]
    pub(super) fn run_mkdir_job(path: OsString, mode: i32) -> AsyncHostResult<i64> {
        mkdir_native_path(path, mode)?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "rmdir_job_worker"
    )]
    pub(super) fn run_rmdir_job(path: OsString) -> AsyncHostResult<i64> {
        rmdir_native_path(path)?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "readdir_job_worker"
    )]
    pub(super) fn run_readdir_job(
        dir: &Resource,
        buffer: &HostCBuffer,
        len: i32,
        restart: bool,
    ) -> AsyncHostResult<i64> {
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let _directory_cursor = dir.lock_directory_cursor();
        let mut buffer = buffer.lock().unwrap();
        let buffer = buffer.get_mut(..len).ok_or(AsyncHostError::Fault)?;
        read_native_dir(dir, buffer, restart)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "realpath_job_worker"
    )]
    pub(super) fn run_realpath_job(
        path: OsString,
        result: &mut Option<RealpathJobResult>,
    ) -> AsyncHostResult<i64> {
        *result = Some(RealpathJobResult::Unpublished(realpath_native_path(path)?));
        Ok(0)
    }
}

#[cfg(unix)]
#[allow(clippy::unnecessary_cast)]
fn open_native_file(
    filename: OsString,
    access: i32,
    create_mode: i32,
    append: bool,
    sync: i32,
    mode: i32,
) -> AsyncHostResult<OpenedFile> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStringExt;

    let access_flag = match access {
        0 | 3 => libc::O_RDONLY,
        1 => libc::O_WRONLY,
        2 => libc::O_RDWR,
        _ => return Err(AsyncHostError::Inval),
    };
    let create_flag = match create_mode {
        0 => 0,
        1 => libc::O_TRUNC,
        2 => libc::O_CREAT,
        3 => libc::O_CREAT | libc::O_TRUNC,
        4 => libc::O_CREAT | libc::O_EXCL,
        _ => return Err(AsyncHostError::Inval),
    };
    let sync_flag = match sync {
        0 => 0,
        1 => libc::O_DSYNC,
        2 => libc::O_SYNC,
        _ => return Err(AsyncHostError::Inval),
    };
    let append_flag = if append { libc::O_APPEND } else { 0 };
    let filename = CString::new(filename.into_vec()).map_err(|_| AsyncHostError::Inval)?;
    let fd = unsafe {
        libc::open(
            filename.as_ptr(),
            access_flag | sync_flag | create_flag | append_flag | libc::O_CLOEXEC,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(last_native_error());
    }

    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } < 0 {
        let error = last_native_error();
        unsafe {
            libc::close(fd);
        }
        return Err(error);
    }
    let stat = unsafe { stat.assume_init() };
    Ok(OpenedFile {
        file: fd,
        kind: file_kind_from_stat(&stat),
        dev_id: stat.st_dev as u64,
        file_id: stat.st_ino as u64,
    })
}

#[cfg(unix)]
fn file_kind_from_stat(stat: &libc::stat) -> i32 {
    match stat.st_mode & libc::S_IFMT {
        libc::S_IFREG => 1,
        libc::S_IFDIR => 2,
        libc::S_IFLNK => 3,
        libc::S_IFSOCK => 4,
        libc::S_IFIFO => 5,
        libc::S_IFBLK => 6,
        libc::S_IFCHR => 7,
        _ => 0,
    }
}

#[cfg(windows)]
fn open_native_file(
    filename: OsString,
    access: i32,
    create_mode: i32,
    append: bool,
    sync: i32,
    _mode: i32,
) -> AsyncHostResult<OpenedFile> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_PIPE_BUSY, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CREATE_ALWAYS, CREATE_NEW, CreateFileW, FILE_APPEND_DATA,
        FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED,
        FILE_FLAG_WRITE_THROUGH, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, GetFileInformationByHandle, OPEN_ALWAYS, OPEN_EXISTING,
        TRUNCATE_EXISTING,
    };
    use windows_sys::Win32::System::Pipes::{NMPWAIT_WAIT_FOREVER, WaitNamedPipeW};

    let sync_flag = match sync {
        0 => 0,
        1 | 2 => FILE_FLAG_WRITE_THROUGH,
        _ => return Err(AsyncHostError::Inval),
    };
    let mut access_flag = match access {
        0 => GENERIC_READ,
        1 => GENERIC_WRITE,
        2 => GENERIC_READ | GENERIC_WRITE,
        3 => FILE_LIST_DIRECTORY,
        _ => return Err(AsyncHostError::Inval),
    };
    let create_mode = match create_mode {
        0 => OPEN_EXISTING,
        1 => TRUNCATE_EXISTING,
        2 => OPEN_ALWAYS,
        3 => CREATE_ALWAYS,
        4 => CREATE_NEW,
        _ => return Err(AsyncHostError::Inval),
    };
    if append {
        access_flag = (access_flag ^ GENERIC_WRITE) | FILE_APPEND_DATA;
    }
    let mut flags = FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS | sync_flag;
    if access == 3 {
        flags |= FILE_FLAG_OVERLAPPED;
    }

    let filename = filename
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let handle: HANDLE = loop {
        let handle = unsafe {
            CreateFileW(
                filename.as_ptr(),
                access_flag,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                create_mode,
                flags,
                std::ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            break handle;
        }
        let error = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno());
        if error != ERROR_PIPE_BUSY as i32 {
            return Err(AsyncHostError::Native(error));
        }
        if unsafe { WaitNamedPipeW(filename.as_ptr(), NMPWAIT_WAIT_FOREVER) } == 0 {
            return Err(last_native_error());
        }
    };

    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    if unsafe { GetFileInformationByHandle(handle, info.as_mut_ptr()) } == 0 {
        let error = last_native_error();
        unsafe {
            CloseHandle(handle);
        }
        return Err(error);
    }
    let info = unsafe { info.assume_init() };
    Ok(OpenedFile {
        file: handle,
        kind: file_kind_from_attr(info.dwFileAttributes),
        dev_id: u64::from(info.dwVolumeSerialNumber),
        file_id: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    })
}

#[cfg(windows)]
fn file_kind_from_attr(attrs: u32) -> i32 {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    };

    if (attrs & FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
        FILE_KIND_SYMLINK
    } else if (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0 {
        FILE_KIND_DIRECTORY
    } else {
        FILE_KIND_REGULAR
    }
}

#[cfg(windows)]
const FILE_KIND_UNKNOWN: i32 = 0;
#[cfg(windows)]
const FILE_KIND_REGULAR: i32 = 1;
#[cfg(windows)]
const FILE_KIND_DIRECTORY: i32 = 2;
#[cfg(windows)]
const FILE_KIND_SYMLINK: i32 = 3;
#[cfg(windows)]
const FILE_KIND_SOCKET: i32 = 4;
#[cfg(windows)]
const FILE_KIND_PIPE: i32 = 5;
#[cfg(windows)]
const FILE_KIND_CHAR_DEVICE: i32 = 7;

#[cfg(windows)]
fn kind_of_raw_file(handle: RawFile) -> AsyncHostResult<i32> {
    use windows_sys::Win32::Foundation::{GetLastError, SetLastError};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_BASIC_INFO, FILE_TYPE_CHAR, FILE_TYPE_DISK, FILE_TYPE_PIPE, FILE_TYPE_UNKNOWN,
        FileBasicInfo, GetFileInformationByHandleEx, GetFileType,
    };

    unsafe {
        SetLastError(0);
    }
    match unsafe { GetFileType(handle) } {
        FILE_TYPE_DISK => {
            let mut info = std::mem::MaybeUninit::<FILE_BASIC_INFO>::uninit();
            if unsafe {
                GetFileInformationByHandleEx(
                    handle,
                    FileBasicInfo,
                    info.as_mut_ptr().cast(),
                    std::mem::size_of::<FILE_BASIC_INFO>() as u32,
                )
            } == 0
            {
                Err(last_native_error())
            } else {
                Ok(file_kind_from_attr(unsafe {
                    info.assume_init().FileAttributes
                }))
            }
        }
        FILE_TYPE_CHAR => Ok(FILE_KIND_CHAR_DEVICE),
        FILE_TYPE_PIPE => {
            if handle_is_socket(handle) {
                Ok(FILE_KIND_SOCKET)
            } else {
                Ok(FILE_KIND_PIPE)
            }
        }
        FILE_TYPE_UNKNOWN => {
            let get_file_type_error = unsafe { GetLastError() };
            if handle_is_socket(handle) {
                Ok(FILE_KIND_SOCKET)
            } else if get_file_type_error == 0 {
                Ok(FILE_KIND_UNKNOWN)
            } else {
                unsafe {
                    SetLastError(get_file_type_error);
                }
                Err(last_native_error())
            }
        }
        _ => Ok(FILE_KIND_UNKNOWN),
    }
}

#[cfg(windows)]
fn handle_is_socket(handle: RawFile) -> bool {
    use windows_sys::Win32::Networking::WinSock::{SO_TYPE, SOCKET, SOL_SOCKET, getsockopt};

    let mut opt = 0i32;
    let mut opt_len = std::mem::size_of::<i32>() as i32;
    unsafe {
        getsockopt(
            handle as SOCKET,
            SOL_SOCKET,
            SO_TYPE,
            (&mut opt as *mut i32).cast(),
            &mut opt_len,
        ) == 0
    }
}

struct OpenedFile {
    file: RawFile,
    kind: i32,
    dev_id: u64,
    file_id: u64,
}

#[cfg(unix)]
fn read_from_native_file(fd: RawFile, buf: &mut [u8], position: i64) -> AsyncHostResult<usize> {
    let ret = if position < 0 {
        loop {
            let ret = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
            if ret >= 0 {
                break ret;
            }
            let errno = std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or_else(|| AsyncHostError::Inval.errno());
            if errno != libc::EAGAIN && errno != libc::EWOULDBLOCK {
                break ret;
            }
            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            if unsafe { libc::poll(&mut pfd, 1, -1) } < 0 {
                break -1;
            }
        }
    } else {
        unsafe {
            libc::pread(
                fd,
                buf.as_mut_ptr().cast(),
                buf.len(),
                position as libc::off_t,
            )
        }
    };
    native_io_result(ret)
}

#[cfg(unix)]
fn write_to_native_file(fd: RawFile, data: &[u8], position: i64) -> AsyncHostResult<usize> {
    let ret = if position < 0 {
        loop {
            let ret = unsafe { libc::write(fd, data.as_ptr().cast(), data.len()) };
            if ret >= 0 {
                break ret;
            }
            let errno = std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or_else(|| AsyncHostError::Inval.errno());
            if errno != libc::EAGAIN && errno != libc::EWOULDBLOCK {
                break ret;
            }
            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLOUT,
                revents: 0,
            };
            if unsafe { libc::poll(&mut pfd, 1, -1) } < 0 {
                break -1;
            }
        }
    } else {
        unsafe {
            libc::pwrite(
                fd,
                data.as_ptr().cast(),
                data.len(),
                position as libc::off_t,
            )
        }
    };
    native_io_result(ret)
}

#[cfg(unix)]
fn file_kind_by_path(
    parent: Option<&Resource>,
    path: OsString,
    follow_symlink: bool,
) -> AsyncHostResult<i32> {
    let path = path_to_cstring(path)?;
    let flags = if follow_symlink {
        0
    } else {
        libc::AT_SYMLINK_NOFOLLOW
    };
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    let parent = match parent {
        Some(parent) => raw_file_handle(parent)?,
        None => libc::AT_FDCWD,
    };
    let ret = unsafe { libc::fstatat(parent, path.as_ptr(), stat.as_mut_ptr(), flags) };
    if ret < 0 {
        Err(last_native_error())
    } else {
        Ok(file_kind_from_stat(&unsafe { stat.assume_init() }))
    }
}

#[cfg(unix)]
#[allow(clippy::unnecessary_cast)]
fn file_size(fd: RawFile) -> AsyncHostResult<i64> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } < 0 {
        return Err(last_native_error());
    }
    Ok(unsafe { stat.assume_init() }.st_size as i64)
}

#[cfg(unix)]
fn file_time(fd: RawFile) -> AsyncHostResult<fd_util::stub::FileTime> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } < 0 {
        return Err(last_native_error());
    }
    Ok(unsafe { stat.assume_init() })
}

#[cfg(unix)]
fn file_time_by_path(
    path: OsString,
    follow_symlink: bool,
) -> AsyncHostResult<fd_util::stub::FileTime> {
    let path = path_to_cstring(path)?;
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    let ret = if follow_symlink {
        unsafe { libc::stat(path.as_ptr(), stat.as_mut_ptr()) }
    } else {
        unsafe { libc::lstat(path.as_ptr(), stat.as_mut_ptr()) }
    };
    if ret < 0 {
        return Err(last_native_error());
    }
    Ok(unsafe { stat.assume_init() })
}

#[cfg(unix)]
fn access_native_path(path: OsString, access: i32) -> AsyncHostResult<()> {
    let path = path_to_cstring(path)?;
    let mode = match access {
        0 => libc::F_OK,
        1 => libc::R_OK,
        2 => libc::W_OK,
        3 => libc::X_OK,
        _ => return Err(AsyncHostError::Inval),
    };
    if unsafe { libc::access(path.as_ptr(), mode) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn chmod_native_path(path: OsString, mode: i32) -> AsyncHostResult<()> {
    let path = path_to_cstring(path)?;
    if unsafe { libc::chmod(path.as_ptr(), mode as libc::mode_t) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sync_native_file(fd: RawFile, only_data: bool) -> AsyncHostResult<()> {
    #[cfg(target_os = "macos")]
    let ret = {
        let _ = only_data;
        unsafe { libc::fsync(fd) }
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let ret = unsafe {
        if only_data {
            libc::fdatasync(fd)
        } else {
            libc::fsync(fd)
        }
    };

    if ret < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn lock_native_file(fd: RawFile, exclusive: bool) -> AsyncHostResult<()> {
    let operation = if exclusive {
        libc::LOCK_EX
    } else {
        libc::LOCK_SH
    };
    if unsafe { libc::flock(fd, operation) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn remove_native_path(path: OsString) -> AsyncHostResult<()> {
    let path = path_to_cstring(path)?;
    if unsafe { libc::remove(path.as_ptr()) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn symlink_native_path(
    target: OsString,
    path: OsString,
    _force_symlink: bool,
) -> AsyncHostResult<()> {
    let target = path_to_cstring(target)?;
    let path = path_to_cstring(path)?;
    if unsafe { libc::symlink(target.as_ptr(), path.as_ptr()) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn mkdir_native_path(path: OsString, mode: i32) -> AsyncHostResult<()> {
    let path = path_to_cstring(path)?;
    if unsafe { libc::mkdir(path.as_ptr(), mode as libc::mode_t) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn rmdir_native_path(path: OsString) -> AsyncHostResult<()> {
    let path = path_to_cstring(path)?;
    if unsafe { libc::rmdir(path.as_ptr()) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn realpath_native_path(path: OsString) -> AsyncHostResult<Box<[u8]>> {
    let path = path_to_cstring(path)?;
    let resolved = unsafe { libc::realpath(path.as_ptr(), std::ptr::null_mut()) };
    if resolved.is_null() {
        return Err(last_native_error());
    }

    let bytes = unsafe { std::ffi::CStr::from_ptr(resolved) }
        .to_bytes_with_nul()
        .to_vec()
        .into_boxed_slice();
    unsafe {
        libc::free(resolved.cast());
    }
    Ok(bytes)
}

#[cfg(all(unix, target_os = "linux"))]
fn read_native_dir(file: &Resource, out: &mut [u8], restart: bool) -> AsyncHostResult<i64> {
    let fd = raw_file_handle(file)?;
    if restart && unsafe { libc::lseek(fd, 0, libc::SEEK_SET) } < 0 {
        return Err(last_native_error());
    }

    let ret = unsafe { libc::syscall(libc::SYS_getdents64, fd, out.as_mut_ptr(), out.len()) };
    if ret < 0 {
        return Err(last_native_error());
    }
    #[cfg(not(target_pointer_width = "64"))]
    let ret = i64::from(ret);
    Ok(ret)
}

#[cfg(all(unix, target_os = "macos"))]
fn read_native_dir(file: &Resource, out: &mut [u8], restart: bool) -> AsyncHostResult<i64> {
    let fd = raw_file_handle(file)?;
    if restart && unsafe { libc::lseek(fd, 0, libc::SEEK_SET) } < 0 {
        return Err(last_native_error());
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
            fd,
            (&mut attr_spec as *mut libc::attrlist).cast(),
            out.as_mut_ptr().cast(),
            out.len(),
            0,
        )
    };
    if ret < 0 {
        return Err(last_native_error());
    }
    Ok(i64::from(ret))
}

#[cfg(all(unix, target_os = "linux"))]
fn rename_native_path(
    old_path: OsString,
    new_path: OsString,
    replace: bool,
) -> AsyncHostResult<()> {
    let old_path = path_to_cstring(old_path)?;
    let new_path = path_to_cstring(new_path)?;
    let flags = if replace { 0 } else { libc::RENAME_NOREPLACE };
    let ret = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            old_path.as_ptr(),
            libc::AT_FDCWD,
            new_path.as_ptr(),
            flags,
        )
    };
    if ret < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(all(unix, target_os = "macos"))]
fn rename_native_path(
    old_path: OsString,
    new_path: OsString,
    replace: bool,
) -> AsyncHostResult<()> {
    let old_path = path_to_cstring(old_path)?;
    let new_path = path_to_cstring(new_path)?;
    let flags = if replace { 0 } else { libc::RENAME_EXCL };
    let ret = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            old_path.as_ptr(),
            libc::AT_FDCWD,
            new_path.as_ptr(),
            flags,
        )
    };
    if ret < 0 {
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

#[cfg(unix)]
fn native_io_result(ret: libc::ssize_t) -> AsyncHostResult<usize> {
    if ret < 0 {
        Err(last_native_error())
    } else {
        usize::try_from(ret).map_err(|_| AsyncHostError::Fault)
    }
}

#[cfg(windows)]
fn read_from_native_file(handle: RawFile, buf: &mut [u8], position: i64) -> AsyncHostResult<usize> {
    use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF, HANDLE};
    use windows_sys::Win32::Storage::FileSystem::ReadFile;
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let overlapped = std::mem::MaybeUninit::<OVERLAPPED>::zeroed();
    let overlapped = unsafe {
        let mut overlapped = overlapped.assume_init();
        if position > 0 {
            overlapped.Anonymous.Anonymous.Offset = position as u32;
            overlapped.Anonymous.Anonymous.OffsetHigh = (position >> 32) as u32;
        }
        overlapped
    };
    let mut overlapped = overlapped;
    let mut bytes_transferred = 0;
    let overlapped_ptr = if position < 0 {
        std::ptr::null_mut()
    } else {
        &mut overlapped
    };
    let handle = handle as HANDLE;
    // Synchronous Windows file handles can advance the current file pointer
    // even when ReadFile receives an OVERLAPPED offset. Keep read_at/pread
    // semantics by restoring the original pointer before returning.
    let saved_position = if position < 0 {
        None
    } else {
        Some(current_file_pointer(handle)?)
    };
    let result = unsafe {
        ReadFile(
            handle,
            buf.as_mut_ptr().cast(),
            u32::try_from(buf.len()).map_err(|_| AsyncHostError::Fault)?,
            &mut bytes_transferred,
            overlapped_ptr,
        )
    };
    let result = if result != 0 {
        usize::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault)
    } else {
        let error = std::io::Error::last_os_error();
        let is_eof = matches!(
            error.raw_os_error(),
            Some(errno) if errno == ERROR_HANDLE_EOF as i32 || errno == ERROR_BROKEN_PIPE as i32
        );
        if is_eof {
            Ok(0)
        } else {
            Err(native_io_error(error))
        }
    };
    if let Some(saved_position) = saved_position
        && let Err(restore_error) = restore_file_pointer(handle, saved_position)
    {
        return match result {
            Ok(_) => Err(restore_error),
            Err(error) => Err(error),
        };
    }
    result
}

#[cfg(windows)]
fn write_to_native_file(handle: RawFile, data: &[u8], position: i64) -> AsyncHostResult<usize> {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::WriteFile;
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let overlapped = std::mem::MaybeUninit::<OVERLAPPED>::zeroed();
    let overlapped = unsafe {
        let mut overlapped = overlapped.assume_init();
        if position > 0 {
            overlapped.Anonymous.Anonymous.Offset = position as u32;
            overlapped.Anonymous.Anonymous.OffsetHigh = (position >> 32) as u32;
        }
        overlapped
    };
    let mut overlapped = overlapped;
    let mut bytes_transferred = 0;
    let overlapped_ptr = if position < 0 {
        std::ptr::null_mut()
    } else {
        &mut overlapped
    };
    let handle = handle as HANDLE;
    // See read_from_native_file: positioned writes must not alter the stream
    // offset seen by following non-positioned writes.
    let saved_position = if position < 0 {
        None
    } else {
        Some(current_file_pointer(handle)?)
    };
    let result = unsafe {
        WriteFile(
            handle,
            data.as_ptr().cast(),
            u32::try_from(data.len()).map_err(|_| AsyncHostError::Fault)?,
            &mut bytes_transferred,
            overlapped_ptr,
        )
    };
    let result = if result != 0 {
        usize::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault)
    } else {
        Err(last_native_error())
    };
    if let Some(saved_position) = saved_position
        && let Err(restore_error) = restore_file_pointer(handle, saved_position)
    {
        return match result {
            Ok(_) => Err(restore_error),
            Err(error) => Err(error),
        };
    }
    result
}

#[cfg(windows)]
fn current_file_pointer(handle: windows_sys::Win32::Foundation::HANDLE) -> AsyncHostResult<i64> {
    use windows_sys::Win32::Storage::FileSystem::{FILE_CURRENT, SetFilePointerEx};

    let mut current = 0;
    if unsafe { SetFilePointerEx(handle, 0, &mut current, FILE_CURRENT) } == 0 {
        Err(last_native_error())
    } else {
        Ok(current)
    }
}

#[cfg(windows)]
fn restore_file_pointer(
    handle: windows_sys::Win32::Foundation::HANDLE,
    position: i64,
) -> AsyncHostResult<()> {
    use windows_sys::Win32::Storage::FileSystem::{FILE_BEGIN, SetFilePointerEx};

    if unsafe { SetFilePointerEx(handle, position, std::ptr::null_mut(), FILE_BEGIN) } == 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn file_kind_by_path(
    parent: Option<&Resource>,
    path: OsString,
    follow_symlink: bool,
) -> AsyncHostResult<i32> {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let mut flags = FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS;
    if !follow_symlink {
        flags |= FILE_FLAG_OPEN_REPARSE_POINT;
    }
    let path = resolve_windows_path_for_parent(parent, path)?;
    let path = path_to_wide(path);
    let handle: HANDLE = unsafe {
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
        return Err(last_native_error());
    }
    let kind = kind_of_raw_file(handle);
    unsafe {
        CloseHandle(handle);
    }
    kind
}

#[cfg(windows)]
fn resolve_windows_path_for_parent(
    parent: Option<&Resource>,
    path: OsString,
) -> AsyncHostResult<OsString> {
    let Some(parent) = parent else {
        return Ok(path);
    };
    if std::path::Path::new(&path).is_absolute() {
        return Ok(path);
    }

    let mut parent_path =
        std::path::PathBuf::from(final_path_from_handle(raw_file_handle(parent)?)?);
    parent_path.push(path);
    Ok(parent_path.into_os_string())
}

#[cfg(windows)]
fn final_path_from_handle(handle: RawFile) -> AsyncHostResult<OsString> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW, VOLUME_NAME_DOS,
    };

    let mut buffer = vec![0u16; 260];
    loop {
        let len = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                buffer.as_mut_ptr(),
                u32::try_from(buffer.len()).map_err(|_| AsyncHostError::Fault)?,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            )
        };
        if len == 0 {
            return Err(last_native_error());
        }
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        if len < buffer.len() {
            buffer.truncate(len);
            return Ok(OsString::from_wide(&buffer));
        }
        buffer.resize(len + 1, 0);
    }
}

#[cfg(windows)]
fn file_size(file: RawFile) -> AsyncHostResult<i64> {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::GetFileSizeEx;

    let mut size = std::mem::MaybeUninit::<i64>::uninit();
    let result = unsafe { GetFileSizeEx(file as HANDLE, size.as_mut_ptr()) };
    if result == 0 {
        Err(last_native_error())
    } else {
        Ok(unsafe { size.assume_init() })
    }
}

#[cfg(windows)]
fn file_time(file: RawFile) -> AsyncHostResult<fd_util::stub::FileTime> {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_BASIC_INFO, FileBasicInfo, GetFileInformationByHandleEx,
    };

    let mut info = std::mem::MaybeUninit::<FILE_BASIC_INFO>::uninit();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file as HANDLE,
            FileBasicInfo,
            info.as_mut_ptr().cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(last_native_error());
    }
    Ok(unsafe { info.assume_init() })
}

#[cfg(windows)]
fn file_time_by_path(
    path: OsString,
    follow_symlink: bool,
) -> AsyncHostResult<fd_util::stub::FileTime> {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_BASIC_INFO, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FileBasicInfo, GetFileInformationByHandleEx, OPEN_EXISTING,
    };

    let mut flags = FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS;
    if !follow_symlink {
        flags |= FILE_FLAG_OPEN_REPARSE_POINT;
    }
    let path = path_to_wide(path);
    let handle: HANDLE = unsafe {
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
        return Err(last_native_error());
    }

    let mut info = std::mem::MaybeUninit::<FILE_BASIC_INFO>::uninit();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileBasicInfo,
            info.as_mut_ptr().cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    };
    unsafe {
        CloseHandle(handle);
    }
    if ok == 0 {
        return Err(last_native_error());
    }
    Ok(unsafe { info.assume_init() })
}

#[cfg(windows)]
fn access_native_path(path: OsString, access: i32) -> AsyncHostResult<()> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_EXECUTE, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let access_mode = match access {
        0 => 0,
        1 => GENERIC_READ,
        2 => GENERIC_WRITE,
        3 => FILE_EXECUTE,
        _ => return Err(AsyncHostError::Inval),
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
        Err(last_native_error())
    } else {
        unsafe {
            CloseHandle(handle);
        }
        Ok(())
    }
}

#[cfg(windows)]
fn chmod_native_path(_path: OsString, _mode: i32) -> AsyncHostResult<()> {
    Err(AsyncHostError::Native(
        windows_sys::Win32::Foundation::ERROR_NOT_SUPPORTED as i32,
    ))
}

#[cfg(windows)]
fn sync_native_file(file: RawFile, _only_data: bool) -> AsyncHostResult<()> {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::FlushFileBuffers;

    if unsafe { FlushFileBuffers(file as HANDLE) } == 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn lock_native_file(file: RawFile, exclusive: bool) -> AsyncHostResult<()> {
    use std::mem::zeroed;
    use windows_sys::Win32::Storage::FileSystem::{LOCKFILE_EXCLUSIVE_LOCK, LockFileEx};
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let mut overlapped: OVERLAPPED = unsafe { zeroed() };
    // Keep parity with thread_pool.c: lock a one-byte sentinel range beyond
    // ordinary file data so Windows mandatory locks approximate advisory locks.
    overlapped.Anonymous.Anonymous.Offset = 0xfffffffe;
    overlapped.Anonymous.Anonymous.OffsetHigh = 0xffffffff;
    let flags = if exclusive {
        LOCKFILE_EXCLUSIVE_LOCK
    } else {
        0
    };
    if unsafe { LockFileEx(file, flags, 0, 1, 0, &mut overlapped) } == 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn remove_native_path(path: OsString) -> AsyncHostResult<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        DeleteFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, GetFileAttributesW,
        INVALID_FILE_ATTRIBUTES, RemoveDirectoryW,
    };

    let path = path_to_wide(path);
    let attrs = unsafe { GetFileAttributesW(path.as_ptr()) };
    if attrs == INVALID_FILE_ATTRIBUTES {
        return Err(last_native_error());
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
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn symlink_native_path(
    target: OsString,
    path: OsString,
    force_symlink: bool,
) -> AsyncHostResult<()> {
    use windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateSymbolicLinkW, FILE_ATTRIBUTE_DIRECTORY, GetFileAttributesW, INVALID_FILE_ATTRIBUTES,
        SYMBOLIC_LINK_FLAG_DIRECTORY,
    };

    let target_wide = path_to_wide(target.clone());
    let path_wide = path_to_wide(path.clone());
    let attrs = unsafe { GetFileAttributesW(target_wide.as_ptr()) };
    let is_directory = attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0;

    if !force_symlink
        && is_directory
        && let Some(junction_target) = junction_target_path(target.clone())
    {
        match create_junction_native_path(junction_target, path.clone()) {
            Ok(()) => return Ok(()),
            Err(AsyncHostError::Native(error)) if error == ERROR_INVALID_PARAMETER as i32 => {}
            Err(error) => return Err(error),
        }
    }

    let flags = if is_directory {
        SYMBOLIC_LINK_FLAG_DIRECTORY
    } else {
        0
    };
    if unsafe { CreateSymbolicLinkW(path_wide.as_ptr(), target_wide.as_ptr(), flags) } != 0 {
        Ok(())
    } else {
        Err(last_native_error())
    }
}

#[cfg(windows)]
fn create_junction_native_path(target: OsString, path: OsString) -> AsyncHostResult<()> {
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
        return Err(last_native_error());
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
        let error = last_native_error();
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
    unsafe {
        CloseHandle(handle);
    }
    if ok == 0 {
        let error = last_native_error();
        unsafe {
            RemoveDirectoryW(path_wide.as_ptr());
        }
        Err(error)
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn junction_target_path(target: OsString) -> Option<OsString> {
    use std::os::windows::ffi::OsStringExt;

    let mut target = os_string_to_wide(target);
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
fn junction_reparse_buffer(target: OsString) -> AsyncHostResult<Vec<u8>> {
    use windows_sys::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;

    const MOUNT_POINT_HEADER_LEN: usize = 8;
    const WCHAR_SIZE: usize = 2;
    const UNICODE_NULL_SIZE: usize = WCHAR_SIZE;
    const NON_INTERPRETED_PATH_PREFIX: [u16; 4] =
        ['\\' as u16, '?' as u16, '?' as u16, '\\' as u16];

    let mut print_name = os_string_to_wide(target);
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
        .ok_or(AsyncHostError::Inval)?;
    let substitute_len = u16::try_from(substitute_len).map_err(|_| AsyncHostError::Inval)?;
    let print_name_len = print_name
        .len()
        .checked_mul(WCHAR_SIZE)
        .ok_or(AsyncHostError::Inval)?;
    let print_name_len = u16::try_from(print_name_len).map_err(|_| AsyncHostError::Inval)?;
    let print_name_offset = substitute_len
        .checked_add(UNICODE_NULL_SIZE as u16)
        .ok_or(AsyncHostError::Inval)?;
    let reparse_data_len = (MOUNT_POINT_HEADER_LEN as u16)
        .checked_add(print_name_offset)
        .and_then(|len| len.checked_add(print_name_len))
        .and_then(|len| len.checked_add(UNICODE_NULL_SIZE as u16))
        .ok_or(AsyncHostError::Inval)?;

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

#[cfg(windows)]
fn mkdir_native_path(path: OsString, _mode: i32) -> AsyncHostResult<()> {
    use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;

    let path = path_to_wide(path);
    if unsafe { CreateDirectoryW(path.as_ptr(), std::ptr::null()) } == 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn rmdir_native_path(path: OsString) -> AsyncHostResult<()> {
    use windows_sys::Win32::Storage::FileSystem::RemoveDirectoryW;

    let path = path_to_wide(path);
    if unsafe { RemoveDirectoryW(path.as_ptr()) } == 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn realpath_native_path(path: OsString) -> AsyncHostResult<Box<[u8]>> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
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
        return Err(last_native_error());
    }

    let result = (|| {
        let flags = FILE_NAME_NORMALIZED | VOLUME_NAME_DOS;
        let mut stack_buffer = [0u16; BUFFER_LEN];
        let len = unsafe {
            GetFinalPathNameByHandleW(file, stack_buffer.as_mut_ptr(), BUFFER_LEN as u32, flags)
        };
        if len == 0 {
            return Err(last_native_error());
        }

        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        if len < stack_buffer.len() {
            return Ok(copy_windows_realpath_c_string_bytes(&stack_buffer[..len]));
        }

        let buffer_len = len.checked_add(1).ok_or(AsyncHostError::Fault)?;
        let mut heap_buffer = vec![0u16; buffer_len];
        let len = unsafe {
            GetFinalPathNameByHandleW(
                file,
                heap_buffer.as_mut_ptr(),
                u32::try_from(heap_buffer.len()).map_err(|_| AsyncHostError::Fault)?,
                flags,
            )
        };
        if len == 0 {
            return Err(last_native_error());
        }
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let units = heap_buffer.get(..len).ok_or(AsyncHostError::Fault)?;
        Ok(copy_windows_realpath_c_string_bytes(units))
    })();

    unsafe {
        CloseHandle(file);
    }
    result
}

#[cfg(windows)]
fn read_native_dir(file: &Resource, out: &mut [u8], restart: bool) -> AsyncHostResult<i64> {
    use windows_sys::Win32::Foundation::{ERROR_NO_MORE_FILES, HANDLE};
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
            raw_file_handle(file)? as HANDLE,
            info_class,
            out.as_mut_ptr().cast(),
            u32::try_from(out.len()).map_err(|_| AsyncHostError::Fault)?,
        )
    };
    if ok == 0 {
        let error = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno());
        if error == ERROR_NO_MORE_FILES as i32 {
            return Ok(0);
        }
        return Err(AsyncHostError::Native(error));
    }

    i64::try_from(out.len()).map_err(|_| AsyncHostError::Fault)
}

#[cfg(windows)]
fn rename_native_path(
    old_path: OsString,
    new_path: OsString,
    replace: bool,
) -> AsyncHostResult<()> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_INVALID_PARAMETER, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, DELETE, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_RENAME_INFO,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileRenameInfoEx,
        MOVEFILE_COPY_ALLOWED, MOVEFILE_REPLACE_EXISTING, MoveFileExW, OPEN_EXISTING,
        SetFileInformationByHandle,
    };

    let old_path = path_to_wide(old_path);
    let new_path = path_to_wide(new_path);
    let handle: HANDLE = unsafe {
        CreateFileW(
            old_path.as_ptr(),
            DELETE,
            FILE_SHARE_DELETE | FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(last_native_error());
    }

    let filename_len = new_path.len().checked_sub(1).ok_or(AsyncHostError::Inval)?;
    let buffer_size = std::mem::size_of::<FILE_RENAME_INFO>()
        .checked_add(filename_len * std::mem::size_of::<u16>())
        .ok_or(AsyncHostError::Fault)?;
    let mut buffer = vec![0u8; buffer_size];
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
        SetFileInformationByHandle(handle, FileRenameInfoEx, info.cast(), buffer_size as u32)
    };
    unsafe {
        CloseHandle(handle);
    }
    if ret != 0 {
        return Ok(());
    }

    let error = std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_else(|| AsyncHostError::Inval.errno());
    if error != ERROR_INVALID_PARAMETER as i32 {
        return Err(AsyncHostError::Native(error));
    }

    let flags = MOVEFILE_COPY_ALLOWED
        | if replace {
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

#[cfg(windows)]
fn path_to_wide(path: OsString) -> Vec<u16> {
    os_string_to_wide(path)
        .into_iter()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn os_string_to_wide(path: OsString) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str().encode_wide().collect()
}

#[cfg(windows)]
fn copy_windows_realpath_c_string_bytes(units: &[u16]) -> Box<[u8]> {
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

    // GetFinalPathNameByHandleW with VOLUME_NAME_DOS returns a `\\?\` path.
    // Match async's user-facing realpath result by normalizing `\\?\C:\...`
    // to `C:\...` and `\\?\UNC\server\...` to `\\server\...`. Checking
    // each complete prefix establishes the slice bounds before rewriting it.
    // The returned string therefore uses conventional rather than verbatim
    // Windows path syntax.
    if units.starts_with(&VERBATIM_UNC_PREFIX) {
        let mut normalized = units[6..].to_vec();
        normalized[0] = '\\' as u16;
        copy_wide_c_string_bytes(&normalized)
    } else if units.starts_with(&VERBATIM_PREFIX) {
        copy_wide_c_string_bytes(&units[VERBATIM_PREFIX.len()..])
    } else {
        // Preserve an unexpected API result rather than dropping arbitrary
        // leading code units.
        copy_wide_c_string_bytes(units)
    }
}

#[cfg(windows)]
fn copy_wide_c_string_bytes(units: &[u16]) -> Box<[u8]> {
    let mut buffer = Vec::with_capacity(std::mem::size_of_val(units) + std::mem::size_of::<u16>());
    for unit in units {
        buffer.extend_from_slice(&unit.to_ne_bytes());
    }
    buffer.extend_from_slice(&0u16.to_ne_bytes());
    buffer.into_boxed_slice()
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}

#[cfg(windows)]
fn native_io_error(error: std::io::Error) -> AsyncHostError {
    AsyncHostError::Native(
        error
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn realpath_normalizes_only_windows_verbatim_prefixes() {
        let normalize = |path: &str| {
            let units = path.encode_utf16().collect::<Vec<_>>();
            let bytes = copy_windows_realpath_c_string_bytes(&units);
            let units = bytes
                .chunks_exact(std::mem::size_of::<u16>())
                .map(|bytes| u16::from_ne_bytes([bytes[0], bytes[1]]))
                .collect::<Vec<_>>();
            assert_eq!(units.last(), Some(&0));
            String::from_utf16(&units[..units.len() - 1]).unwrap()
        };

        assert_eq!(normalize(r"\\?\C:\dir\file"), r"C:\dir\file");
        assert_eq!(
            normalize(r"\\?\UNC\server\share\file"),
            r"\\server\share\file"
        );
        assert_eq!(normalize(r"C:\already-normal"), r"C:\already-normal");
    }

    #[test]
    fn junction_reparse_buffer_encodes_substitute_and_print_names() {
        let data = junction_reparse_buffer(OsString::from("C:/target")).unwrap();
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
