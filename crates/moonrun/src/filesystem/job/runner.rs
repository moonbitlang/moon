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

//! Concrete Host Filesystem operations executed by Jobs.
//!
//! Each executor interprets and authorizes its inputs before consuming acquired
//! references or applying native effects. The helpers below implement mechanism;
//! Job dispatch does not perform a separate permission or pathname pass.

use std::ffi::OsString;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use super::last_native_error;
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::fd_util;
use crate::async_sys::ported_fns;
use crate::resource::{Resource, ResourcePublication, ResourceRef};

#[cfg(windows)]
use super::stat::{
    FILE_KIND_CHAR_DEVICE, FILE_KIND_DIRECTORY, FILE_KIND_PIPE, FILE_KIND_REGULAR,
    FILE_KIND_SOCKET, FILE_KIND_SYMLINK, FILE_KIND_UNKNOWN,
};
use super::stat::{PackedStat, StatRequest, StatValues, query_handle};
use super::{FileTimeResult, OpenJobResult, RealpathJobResult, job_path, take_execution_path};
use crate::filesystem::{FileGrant, FsOperation, FsPath, FsTarget, HostFs};

type RawFile = fd_util::stub::RawFd;

#[cfg(unix)]
pub(super) fn raw_file_handle(file: &Resource) -> AsyncHostResult<RawFile> {
    Ok(file.as_file()?.as_raw_fd())
}

#[cfg(windows)]
pub(super) fn raw_file_handle(file: &Resource) -> AsyncHostResult<RawFile> {
    Ok(file.as_file()?.as_raw_handle())
}

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/fs.c",
        original = "openat_job_worker"
    )]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_open_job(
        filesystem: &HostFs,
        parent: &mut Option<ResourceRef>,
        result: &mut Option<OpenJobResult>,
        filename: &mut OsString,
        access: i32,
        create_mode: i32,
        append: bool,
        sync: i32,
        mode: i32,
        request: StatRequest,
    ) -> AsyncHostResult<i64> {
        let granted = filesystem.authorize(FsOperation::OpenAsync {
            path: job_path(parent.as_deref(), filename)?,
            access,
            create_mode,
            read_metadata: request.mask() & !super::STAT_OPEN_IDENTITY != 0,
        })?;
        let filename = take_execution_path(filesystem, parent.as_deref(), filename);
        let parent = parent.take();
        let parent = parent.as_deref();

        let parent_handle = parent.map(raw_file_handle).transpose()?;
        let options = open_options(access, create_mode, append, sync, mode)?;
        let file = super::super::os::openat::open(
            parent_handle,
            &filename,
            &options,
            &super::CancellableIo,
        )?;
        let resource = Resource::from_file(file, FileGrant::acquired(granted));
        // Unlike async Workers, synchronous calls do not mask ordinary signals.
        // An unnecessary stat can therefore return EINTR after open succeeds.
        // Callers cannot tell which syscall was interrupted and may retry open,
        // repeating its side effects.
        let stat = if request.mask() == 0 {
            PackedStat::new(request, StatValues::default())
        } else {
            // Opening already admitted the complete metadata request. This
            // acquisition receipt must not run a second saved-path check.
            PackedStat::new(request, query_handle(&resource, request)?)
        };
        *result = Some(OpenJobResult {
            resource: ResourcePublication::Unpublished(resource),
            stat,
        });
        Ok(0)
    }

    #[ported(runtime = "thread_pool.c", original = "read_job_worker")]
    pub(super) fn run_read_job(
        filesystem: &HostFs,
        file: &mut Option<ResourceRef>,
        len: u32,
        position: i64,
        result: &mut Option<Vec<u8>>,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::ReadResource(
            file.as_deref().ok_or(AsyncHostError::Badf)?.file_grant(),
        ))?;
        let file = file.take().ok_or(AsyncHostError::Badf)?;

        let mut buf = vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?];
        let n = crate::filesystem::os::io::read_blocking(
            raw_file_handle(&file)?, &mut buf, u64::try_from(position).ok(), &super::CancellableIo,
        )?;
        buf.truncate(n);
        *result = Some(buf);
        Ok(n as i64)
    }

    #[ported(runtime = "thread_pool.c", original = "write_job_worker")]
    pub(super) fn run_write_job(
        filesystem: &HostFs,
        file: &mut Option<ResourceRef>,
        data: &[u8],
        position: i64,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::WriteResource(
            file.as_deref().ok_or(AsyncHostError::Badf)?.file_grant(),
        ))?;
        let file = file.take().ok_or(AsyncHostError::Badf)?;

        let n = crate::filesystem::os::io::write_blocking(
            raw_file_handle(&file)?, data, u64::try_from(position).ok(), &super::CancellableIo,
        )?;
        Ok(n as i64)
    }

    #[compat(
        source = "src/internal/event_loop/thread_pool.c",
        original = "file_kind_by_path_job_worker",
        upstream_pr = 527,
        replacement = "statx_job_worker with STAT_FILE_KIND"
    )]
    pub(super) fn run_file_kind_by_path_job(
        filesystem: &HostFs,
        parent: &mut Option<ResourceRef>,
        path: &mut OsString,
        follow_symlink: bool,
    ) -> AsyncHostResult<i64> {
        let target = job_path(parent.as_deref(), path)?;
        filesystem.authorize(if follow_symlink {
            FsOperation::Metadata(FsTarget::Path(target))
        } else {
            FsOperation::InspectEntry(target)
        })?;
        let path = take_execution_path(filesystem, parent.as_deref(), path);
        let parent = parent.take();
        let parent = parent.as_deref();

        file_kind_by_path(parent, path, follow_symlink).map(i64::from)
    }

    #[compat(
        source = "src/internal/event_loop/thread_pool.c",
        original = "file_size_job_worker",
        upstream_pr = 527,
        replacement = "fstatx_job_worker with STAT_FILE_SIZE"
    )]
    pub(super) fn run_file_size_job(
        filesystem: &HostFs,
        file: &mut Option<ResourceRef>,
        result: &mut i64,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::Metadata(FsTarget::File(
            file.as_deref().ok_or(AsyncHostError::Badf)?.file_grant(),
        )))?;
        let file = file.take().ok_or(AsyncHostError::Badf)?;

        *result = file_size(raw_file_handle(&file)?)?;
        Ok(0)
    }

    #[compat(
        source = "src/internal/event_loop/thread_pool.c",
        original = "file_time_job_worker",
        upstream_pr = 527,
        replacement = "fstatx_job_worker with timestamp properties"
    )]
    pub(super) fn run_file_time_job(
        filesystem: &HostFs,
        file: &mut Option<ResourceRef>,
        result: &mut Option<FileTimeResult>,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::Metadata(FsTarget::File(
            file.as_deref().ok_or(AsyncHostError::Badf)?.file_grant(),
        )))?;
        let file = file.take().ok_or(AsyncHostError::Badf)?;

        *result = Some(FileTimeResult::new(file_time(raw_file_handle(&file)?)?));
        Ok(0)
    }

    #[compat(
        source = "src/internal/event_loop/thread_pool.c",
        original = "file_time_by_path_job_worker",
        upstream_pr = 527,
        replacement = "statx_job_worker with timestamp properties"
    )]
    pub(super) fn run_file_time_by_path_job(
        filesystem: &HostFs,
        path: &mut OsString,
        follow_symlink: bool,
        result: &mut Option<FileTimeResult>,
    ) -> AsyncHostResult<i64> {
        let target = FsPath::Cwd(path);
        filesystem.authorize(if follow_symlink {
            FsOperation::Metadata(FsTarget::Path(target))
        } else {
            FsOperation::InspectEntry(target)
        })?;
        let path = filesystem.resolve_path(&std::mem::take(path));

        *result = Some(FileTimeResult::new(file_time_by_path(
            path,
            follow_symlink,
        )?));
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/fs.c",
        original = "access_job_worker"
    )]
    pub(super) fn run_access_job(
        filesystem: &HostFs,
        path: &mut OsString,
        access: i32,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::AccessCheck {
            path: FsPath::Cwd(path),
            access,
        })?;
        let path = filesystem.resolve_path(&std::mem::take(path));

        access_native_path(path, access)?;
        Ok(0)
    }

    #[ported(source = "src/internal/event_loop/fs.c", original = "chmod_job_worker")]
    pub(super) fn run_chmod_job(
        filesystem: &HostFs,
        path: &mut OsString,
        mode: i32,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::Chmod(FsPath::Cwd(path)))?;
        let path = filesystem.resolve_path(&std::mem::take(path));

        chmod_native_path(path, mode)?;
        Ok(0)
    }

    #[ported(source = "src/internal/event_loop/fs.c", original = "fsync_job_worker")]
    pub(super) fn run_fsync_job(
        filesystem: &HostFs,
        file: &mut Option<ResourceRef>,
        only_data: bool,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::Flush(
            file.as_deref().ok_or(AsyncHostError::Badf)?.file_grant(),
        ))?;
        let file = file.take().ok_or(AsyncHostError::Badf)?;

        sync_native_file(raw_file_handle(&file)?, only_data)?;
        Ok(0)
    }

    #[ported(source = "src/internal/event_loop/fs.c", original = "flock_job_worker")]
    pub(super) fn run_flock_job(
        filesystem: &HostFs,
        file: &mut Option<ResourceRef>,
        exclusive: bool,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::Lock {
            file: file.as_deref().ok_or(AsyncHostError::Badf)?.file_grant(),
            exclusive,
        })?;
        let file = file.take().ok_or(AsyncHostError::Badf)?;

        super::super::os::lock::lock_blocking(
            raw_file_handle(&file)?, exclusive, &super::CancellableIo,
        )?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/fs.c",
        original = "remove_job_worker"
    )]
    pub(super) fn run_remove_job(filesystem: &HostFs, path: &mut OsString) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::ChangeEntry(FsPath::Cwd(path)))?;
        let path = filesystem.resolve_path(&std::mem::take(path));

        remove_native_path(path)?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/fs.c",
        original = "removeat_job_worker"
    )]
    pub(super) fn run_removeat_job(
        filesystem: &HostFs,
        parent: &mut Option<ResourceRef>,
        path: &mut OsString,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::ChangeEntry(job_path(parent.as_deref(), path)?))?;
        let path = take_execution_path(filesystem, parent.as_deref(), path);
        let parent = parent.take();
        let parent = parent.as_deref();

        super::super::os::unlinkat::remove_at(
            parent.map(raw_file_handle).transpose()?,
            &path,
            &super::CancellableIo,
        )?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/fs.c",
        original = "rename_job_worker"
    )]
    pub(super) fn run_rename_job(
        filesystem: &HostFs,
        old_path: &mut OsString,
        new_path: &mut OsString,
        replace: bool,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::Rename {
            source: FsPath::Cwd(old_path),
            destination: FsPath::Cwd(new_path),
        })?;
        let old_path = filesystem.resolve_path(&std::mem::take(old_path));
        let new_path = filesystem.resolve_path(&std::mem::take(new_path));

        crate::filesystem::os::rename(
            std::path::Path::new(&old_path),
            std::path::Path::new(&new_path),
            crate::filesystem::os::RenameOptions {
                replace,
                cross_device: crate::filesystem::os::CrossDevice::AllowCopy,
            },
        )?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/fs.c",
        original = "symlink_job_worker"
    )]
    pub(super) fn run_symlink_job(
        filesystem: &HostFs,
        target: &mut OsString,
        path: &mut OsString,
        force_symlink: bool,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::ChangeEntry(FsPath::Cwd(path)))?;
        let path = filesystem.resolve_path(&std::mem::take(path));

        symlink_native_path(std::mem::take(target), path, force_symlink)?;
        Ok(0)
    }

    #[ported(source = "src/internal/event_loop/fs.c", original = "mkdir_job_worker")]
    pub(super) fn run_mkdir_job(
        filesystem: &HostFs,
        path: &mut OsString,
        mode: i32,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::ChangeEntry(FsPath::Cwd(path)))?;
        let path = filesystem.resolve_path(&std::mem::take(path));

        mkdir_native_path(path, mode)?;
        Ok(0)
    }

    #[ported(source = "src/internal/event_loop/fs.c", original = "rmdir_job_worker")]
    pub(super) fn run_rmdir_job(filesystem: &HostFs, path: &mut OsString) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::ChangeEntry(FsPath::Cwd(path)))?;
        let path = filesystem.resolve_path(&std::mem::take(path));

        rmdir_native_path(path)?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/fs.c",
        original = "readdir_job_worker"
    )]
    pub(super) fn run_readdir_job(
        filesystem: &HostFs,
        dir: &mut Option<ResourceRef>,
        buffer: &mut Option<crate::async_host::CBufferLease>,
        len: u32,
        restart: bool,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::ReadResource(
            dir.as_deref().ok_or(AsyncHostError::Badf)?.file_grant(),
        ))?;
        let dir = dir.take().ok_or(AsyncHostError::Badf)?;
        let buffer = buffer.as_mut().ok_or(AsyncHostError::Badf)?.as_mut_slice();

        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let _directory_cursor = dir.lock_directory_cursor();
        let buffer = buffer.get_mut(..len).ok_or(AsyncHostError::Fault)?;
        read_native_dir(&dir, buffer, restart)
    }

    #[ported(
        source = "src/internal/event_loop/fs.c",
        original = "inotify_add_watch_job_worker"
    )]
    #[cfg(target_os = "linux")]
    pub(super) fn run_inotify_add_watch_job(
        filesystem: &HostFs,
        inotify: &mut Option<ResourceRef>,
        path: &mut OsString,
        is_dir: bool,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::Watch(FsTarget::Path(FsPath::Cwd(path))))?;
        let path = filesystem.resolve_path(&std::mem::take(path));
        let inotify = inotify.take().ok_or(AsyncHostError::Badf)?;

        use std::ffi::CString;
        use std::os::unix::ffi::OsStringExt;

        let path = CString::new(path.into_vec()).map_err(|_| AsyncHostError::Inval)?;
        let flags = if is_dir {
            libc::IN_CREATE | libc::IN_MOVED_FROM | libc::IN_MOVED_TO | libc::IN_DELETE
        } else {
            libc::IN_MODIFY
        };
        let watch =
            unsafe { libc::inotify_add_watch(inotify.as_file()?.as_raw_fd(), path.as_ptr(), flags) };
        if watch < 0 {
            return Err(last_native_error());
        }
        Ok(i64::from(watch))
    }

    #[ported(
        source = "src/internal/event_loop/fs.c",
        original = "realpath_job_worker"
    )]
    pub(super) fn run_realpath_job(
        filesystem: &HostFs,
        path: &mut OsString,
        result: &mut Option<RealpathJobResult>,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::Read(FsPath::Cwd(path)))?;
        let path = filesystem.resolve_path(&std::mem::take(path));

        *result = Some(RealpathJobResult::Unpublished(realpath_native_path(path)?));
        Ok(0)
    }
}

fn open_options(
    access: i32,
    create_mode: i32,
    append: bool,
    sync: i32,
    mode: i32,
) -> AsyncHostResult<super::super::os::openat::OpenOptions> {
    use super::super::os::openat::{Access, CreateMode, OpenOptions, Sync, TypeConstraint};
    let is_async = access == 3;
    let (access, type_constraint) = match access {
        0 => (Access::Read, TypeConstraint::Any),
        1 => (Access::Write, TypeConstraint::Any),
        2 => (Access::ReadWrite, TypeConstraint::Any),
        3 => (Access::ListDirectory, TypeConstraint::Directory),
        _ => return Err(AsyncHostError::Inval),
    };
    Ok(OpenOptions {
        access,
        type_constraint,
        follow_symlink: true,
        is_async,
        create: match create_mode {
            0 => CreateMode::Open,
            1 => CreateMode::Truncate,
            2 => CreateMode::Create,
            3 => CreateMode::CreateOrTruncate,
            4 => CreateMode::Exclusive,
            _ => return Err(AsyncHostError::Inval),
        },
        append,
        sync: match sync {
            0 => Sync::None,
            1 => Sync::Data,
            2 => Sync::All,
            _ => return Err(AsyncHostError::Inval),
        },
        permission: mode as u32,
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
pub(super) fn handle_is_socket(handle: RawFile) -> bool {
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

#[cfg(unix)]
fn path_to_cstring(path: OsString) -> AsyncHostResult<std::ffi::CString> {
    use std::os::unix::ffi::OsStringExt;

    std::ffi::CString::new(path.into_vec()).map_err(|_| AsyncHostError::Inval)
}

#[cfg(windows)]
fn file_kind_by_path(
    parent: Option<&Resource>,
    path: OsString,
    follow_symlink: bool,
) -> AsyncHostResult<i32> {
    use super::super::os::openat::{self, Access, CreateMode, OpenOptions, Sync, TypeConstraint};
    let file = openat::open(
        parent.map(raw_file_handle).transpose()?,
        &path,
        &OpenOptions {
            access: Access::Attributes,
            type_constraint: TypeConstraint::Any,
            follow_symlink,
            is_async: false,
            create: CreateMode::Open,
            append: false,
            sync: Sync::None,
            permission: 0,
        },
        &super::CancellableIo,
    )?;
    kind_of_raw_file(file.as_raw_handle())
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
        CreateSymbolicLinkW, SYMBOLIC_LINK_FLAG_DIRECTORY,
    };

    let target_wide = path_to_wide(target.clone());
    let path_wide = path_to_wide(path.clone());
    let is_directory = symlink_target_is_directory(&target, &path);

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
    if unsafe { CreateSymbolicLinkW(path_wide.as_ptr(), target_wide.as_ptr(), flags) } {
        Ok(())
    } else {
        Err(last_native_error())
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
    let probe = path_to_wide(probe.into_os_string());
    let attrs = unsafe { GetFileAttributesW(probe.as_ptr()) };
    attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0
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

    // Strip an extended prefix only when ordinary DOS parsing preserves every
    // component. Otherwise the returned path could name a different file.
    if units.starts_with(&VERBATIM_UNC_PREFIX) && windows_dos_path_is_exact(&units[8..]) {
        let mut normalized = units[6..].to_vec();
        normalized[0] = '\\' as u16;
        copy_wide_c_string_bytes(&normalized)
    } else if units.starts_with(&VERBATIM_PREFIX)
        && !units.starts_with(&VERBATIM_UNC_PREFIX)
        && windows_dos_path_is_exact(&units[4..])
    {
        copy_wide_c_string_bytes(&units[4..])
    } else {
        copy_wide_c_string_bytes(units)
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

#[cfg(windows)]
fn copy_wide_c_string_bytes(units: &[u16]) -> Box<[u8]> {
    let mut buffer = Vec::with_capacity(std::mem::size_of_val(units) + std::mem::size_of::<u16>());
    for unit in units {
        buffer.extend_from_slice(&unit.to_ne_bytes());
    }
    buffer.extend_from_slice(&0u16.to_ne_bytes());
    buffer.into_boxed_slice()
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
        create_junction_native_path(
            target.clone().into_os_string(),
            old_link.clone().into_os_string(),
        )
        .unwrap();
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
    fn realpath_preserves_windows_path_identity() {
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
