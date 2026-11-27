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

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::resource::{Resource, ResourcePublication, ResourceRef};
use crate::upstream::ported_fns;

use super::stat::{PackedStat, StatRequest};
use super::{FileTimeResult, OpenJobResult, RealpathJobResult, job_path, take_execution_path};
use crate::filesystem::metadata::query_resource_stat;
use crate::filesystem::os::stat::StatValues;
use crate::filesystem::{FileGrant, FsOperation, FsPath, FsTarget, HostFs};

use crate::filesystem::os::{self, RawFile, metadata};

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
        use os::openat::{Access, CreateMode, OpenOptions, Sync, TypeConstraint};
        let access = match access {
            0 => Some(Access::Read),
            1 => Some(Access::Write),
            2 => Some(Access::ReadWrite),
            3 => Some(Access::ListDirectory),
            _ => None,
        };
        let create = match create_mode {
            0 => Some(CreateMode::Open),
            1 => Some(CreateMode::Truncate),
            2 => Some(CreateMode::Create),
            3 => Some(CreateMode::CreateOrTruncate),
            4 => Some(CreateMode::Exclusive),
            _ => None,
        };
        let granted = filesystem.authorize(FsOperation::Open {
            path: job_path(parent.as_deref(), filename)?,
            access,
            create,
            append,
            read_metadata: request.mask() & !super::STAT_OPEN_IDENTITY != 0,
        })?;
        let filename = take_execution_path(filesystem, parent.as_deref(), filename);
        let parent = parent.take();
        let parent = parent.as_deref();

        let parent_handle = parent.map(raw_file_handle).transpose()?;
        let access = access.ok_or(AsyncHostError::Inval)?;
        let list = access == Access::ListDirectory;
        let options = OpenOptions {
            access,
            type_constraint: if list { TypeConstraint::Directory } else { TypeConstraint::Any },
            follow_symlink: true,
            is_async: list,
            create: create.ok_or(AsyncHostError::Inval)?,
            append,
            sync: match sync {
                0 => Sync::None,
                1 => Sync::Data,
                2 => Sync::All,
                _ => return Err(AsyncHostError::Inval),
            },
            permission: mode as u32,
        };
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
            PackedStat::new(request, query_resource_stat(&resource, request.fields())?)
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

        super::super::metadata::legacy::kind_by_path(
            parent.map(raw_file_handle).transpose()?, &path, follow_symlink, &super::CancellableIo,
        ).map(i64::from)
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

        *result = metadata::file_size(raw_file_handle(&file)?)?;
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

        *result = Some(FileTimeResult::new(metadata::file_info(raw_file_handle(&file)?)?));
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

        *result = Some(FileTimeResult::new(metadata::file_info_by_path(
            &path,
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

        let access = match access {
            0 => os::path::AccessMode::Exists,
            1 => os::path::AccessMode::Read,
            2 => os::path::AccessMode::Write,
            3 => os::path::AccessMode::Execute,
            _ => return Err(AsyncHostError::Inval),
        };
        os::path::access(&path, access)?;
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

        os::path::chmod(&path, mode as u32)?;
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

        super::super::os::io::sync(raw_file_handle(&file)?, only_data)?;
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
        filesystem.authorize(FsOperation::RemoveEntry(FsPath::Cwd(path)))?;
        let path = filesystem.resolve_path(&std::mem::take(path));

        os::path::remove(&path)?;
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
        filesystem.authorize(FsOperation::RemoveEntry(job_path(parent.as_deref(), path)?))?;
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
        let options = os::RenameOptions {
            replace,
            cross_device: os::CrossDevice::AllowCopy,
        };
        filesystem.authorize(FsOperation::Rename {
            source: FsPath::Cwd(old_path),
            destination: FsPath::Cwd(new_path),
            options,
        })?;
        let old_path = filesystem.resolve_path(&std::mem::take(old_path));
        let new_path = filesystem.resolve_path(&std::mem::take(new_path));

        os::rename(
            std::path::Path::new(&old_path),
            std::path::Path::new(&new_path),
            options,
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
        filesystem.authorize(FsOperation::CreateSymlink { path: FsPath::Cwd(path), target, force_symlink })?;
        let path = filesystem.resolve_path(&std::mem::take(path));

        os::symlink::create(&std::mem::take(target), &path, force_symlink)?;
        Ok(0)
    }

    #[ported(source = "src/internal/event_loop/fs.c", original = "mkdir_job_worker")]
    pub(super) fn run_mkdir_job(
        filesystem: &HostFs,
        path: &mut OsString,
        mode: i32,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::CreateDirectory { path: FsPath::Cwd(path), mode: Some(mode as u32) })?;
        let path = filesystem.resolve_path(&std::mem::take(path));

        os::path::mkdir(&path, mode as u32)?;
        Ok(0)
    }

    #[ported(source = "src/internal/event_loop/fs.c", original = "rmdir_job_worker")]
    pub(super) fn run_rmdir_job(filesystem: &HostFs, path: &mut OsString) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::RemoveDirectory(FsPath::Cwd(path)))?;
        let path = filesystem.resolve_path(&std::mem::take(path));

        os::path::rmdir(&path)?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/fs.c",
        original = "readdir_job_worker"
    )]
    pub(super) fn run_readdir_job(
        filesystem: &HostFs,
        dir: &mut Option<ResourceRef>,
        buffer: &mut Option<crate::async_host::buffers::CBufferLease>,
        len: u32,
        restart: bool,
    ) -> AsyncHostResult<i64> {
        filesystem.authorize(FsOperation::ListDirectory(FsTarget::File(
            dir.as_deref().ok_or(AsyncHostError::Badf)?.file_grant(),
        )))?;
        let dir = dir.take().ok_or(AsyncHostError::Badf)?;
        let buffer = buffer.as_mut().ok_or(AsyncHostError::Badf)?.as_mut_slice();

        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let _directory_cursor = dir.lock_directory_cursor();
        let buffer = buffer.get_mut(..len).ok_or(AsyncHostError::Fault)?;
        os::directory::read(raw_file_handle(&dir)?, buffer, restart)
            .map(|count| count as i64)
            .map_err(Into::into)
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

        Ok(i64::from(os::watch::add_file(
            inotify.as_file()?.as_raw_fd(),
            &path,
            is_dir,
        )?))
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

        let resolved = os::path::realpath(&path)?;
        // The async ABI publishes a NUL-terminated native string as a C buffer.
        #[cfg(unix)]
        let bytes = {
            use std::os::unix::ffi::OsStringExt;
            let mut bytes = resolved.into_vec();
            bytes.push(0);
            bytes
        };
        #[cfg(windows)]
        let bytes = {
            use std::os::windows::ffi::OsStrExt;
            resolved.encode_wide().chain(std::iter::once(0))
                .flat_map(u16::to_ne_bytes).collect::<Vec<_>>()
        };
        *result = Some(RealpathJobResult::Unpublished(bytes.into_boxed_slice()));
        Ok(0)
    }
}
