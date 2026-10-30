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

//! Filesystem-owned Job state submitted to moonrun's shared thread pool.

mod console;
mod runner;
mod stat;
mod tty;

pub(crate) use stat::stdio_kind;

use std::ffi::{OsStr, OsString};

use crate::async_host::{AsyncHostError, AsyncHostResult, CBufferLease};
use crate::async_sys::internal::fd_util;
use crate::filesystem::HostFs;
use crate::guest_memory::GuestMemory;
use crate::resource::{Resource, ResourcePublication, ResourceRef};

use stat::{PackedStat, STAT_DEVICE_ID, STAT_FILE_ID, STAT_FILE_KIND, StatRequest};

pub(crate) const STAT_OPEN_IDENTITY: u32 = stat::STAT_OPEN_IDENTITY;

#[derive(Debug)]
pub(crate) struct OpenJobResult {
    pub(crate) resource: ResourcePublication,
    stat: PackedStat,
}

#[derive(Clone, Copy)]
struct FileTimeResult(fd_util::stub::FileTime);

impl FileTimeResult {
    fn new(file_time: fd_util::stub::FileTime) -> Self {
        Self(file_time)
    }

    fn as_native(&self) -> &fd_util::stub::FileTime {
        &self.0
    }
}

impl std::fmt::Debug for FileTimeResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("FileTimeResult").finish_non_exhaustive()
    }
}

#[derive(Debug)]
enum RealpathJobResult {
    // The completed Job owns the native path until the guest requests it.
    Unpublished(Box<[u8]>),
    // The host c_buffer table owns the path and the Job finalizer releases it.
    Published(u64),
}

#[derive(Debug)]
pub(crate) struct Job {
    kind: Kind,
}

#[derive(Debug)]
enum Kind {
    Open {
        filename: OsString,
        access: i32,
        create_mode: i32,
        append: bool,
        sync: i32,
        mode: i32,
        request: StatRequest,
        result: Option<OpenJobResult>,
    },
    Fstatx {
        file: Option<ResourceRef>,
        request: StatRequest,
        result: Option<PackedStat>,
    },
    Statx {
        parent: Option<ResourceRef>,
        path: OsString,
        request: StatRequest,
        follow_symlink: bool,
        result: Option<PackedStat>,
    },
    Read {
        file: Option<ResourceRef>,
        len: u32,
        position: i64,
        result: Option<Vec<u8>>,
    },
    Write {
        file: Option<ResourceRef>,
        data: Vec<u8>,
        position: i64,
    },
    ReadConsole {
        file: Option<ResourceRef>,
        len: u32,
        result: Option<Vec<u16>>,
    },
    WriteConsole {
        file: Option<ResourceRef>,
        data: Vec<u16>,
    },
    TtyReadConsoleInput {
        input: Option<ResourceRef>,
        // Set by cancellation, created along with the Job.
        #[cfg(windows)]
        cancel: ResourceRef,
        len: u32,
        result: Option<Vec<u16>>,
        // A resize read along with the returned input.
        resized: bool,
    },
    FileKindByPath {
        parent: Option<ResourceRef>,
        path: OsString,
        follow_symlink: bool,
    },
    FileSize {
        file: Option<ResourceRef>,
        result: i64,
    },
    FileTime {
        file: Option<ResourceRef>,
        result: Option<FileTimeResult>,
    },
    FileTimeByPath {
        path: OsString,
        follow_symlink: bool,
        result: Option<FileTimeResult>,
    },
    Access {
        path: OsString,
        access: i32,
    },
    Chmod {
        path: OsString,
        mode: i32,
    },
    Fsync {
        file: Option<ResourceRef>,
        only_data: bool,
    },
    Flock {
        file: Option<ResourceRef>,
        exclusive: bool,
    },
    Remove {
        path: OsString,
    },
    Rename {
        old_path: OsString,
        new_path: OsString,
        replace: bool,
    },
    Symlink {
        target: OsString,
        path: OsString,
        force_symlink: bool,
    },
    Mkdir {
        path: OsString,
        mode: i32,
    },
    Rmdir {
        path: OsString,
    },
    Readdir {
        dir: Option<ResourceRef>,
        buffer: Option<CBufferLease>,
        len: u32,
        restart: bool,
    },
    #[cfg(target_os = "linux")]
    InotifyAddWatch {
        inotify: Option<ResourceRef>,
        path: OsString,
        is_dir: bool,
    },
    Realpath {
        path: OsString,
        result: Option<RealpathJobResult>,
    },
}

impl Job {
    pub(crate) fn configure_paths(&mut self, filesystem: &HostFs) {
        let resolve = |p: &mut OsString| {
            *p = filesystem.resolve_path(p);
        };
        match &mut self.kind {
            Kind::Open { filename, .. } => resolve(filename),
            Kind::Statx {
                parent: None, path, ..
            }
            | Kind::FileKindByPath {
                parent: None, path, ..
            }
            | Kind::FileTimeByPath { path, .. }
            | Kind::Access { path, .. }
            | Kind::Chmod { path, .. }
            | Kind::Remove { path, .. }
            | Kind::Symlink { path, .. }
            | Kind::Mkdir { path, .. }
            | Kind::Rmdir { path, .. }
            | Kind::Realpath { path, .. } => resolve(path),
            Kind::Rename {
                old_path, new_path, ..
            } => {
                resolve(old_path);
                resolve(new_path);
            }
            #[cfg(target_os = "linux")]
            Kind::InotifyAddWatch { path, .. } => resolve(path),
            _ => {}
        };
    }

    // Generic-stat validation belongs to the Filesystem Job so every adapter
    // constructs the same valid payload. The outer adapter converts rejection
    // into the native failed-Job representation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open(
        filename: OsString,
        access: i32,
        create_mode: i32,
        append: bool,
        sync: i32,
        mode: i32,
        stat_request: u32,
        stat_result_len: u32,
    ) -> AsyncHostResult<Self> {
        Ok(Self {
            kind: Kind::Open {
                filename,
                access,
                create_mode,
                append,
                sync,
                mode,
                request: StatRequest::new(stat_request, stat_result_len)?,
                result: None,
            },
        })
    }

    pub(crate) fn open_legacy(
        filename: OsString,
        access: i32,
        create_mode: i32,
        append: bool,
        sync: i32,
        mode: i32,
    ) -> Self {
        Self {
            kind: Kind::Open {
                filename,
                access,
                create_mode,
                append,
                sync,
                mode,
                request: StatRequest::open_identity(),
                result: None,
            },
        }
    }

    pub(crate) fn fstatx(
        file: ResourceRef,
        stat_request: u32,
        stat_result_len: u32,
    ) -> AsyncHostResult<Self> {
        Ok(Self {
            kind: Kind::Fstatx {
                file: Some(file),
                request: StatRequest::new(stat_request, stat_result_len)?,
                result: None,
            },
        })
    }

    pub(crate) fn statx(
        parent: Option<ResourceRef>,
        path: OsString,
        stat_request: u32,
        stat_result_len: u32,
        follow_symlink: bool,
    ) -> AsyncHostResult<Self> {
        Ok(Self {
            kind: Kind::Statx {
                parent,
                path,
                request: StatRequest::new(stat_request, stat_result_len)?,
                follow_symlink,
                result: None,
            },
        })
    }

    pub(crate) fn read(file: ResourceRef, len: u32, position: i64) -> Self {
        Self {
            kind: Kind::Read {
                file: Some(file),
                len,
                position,
                result: None,
            },
        }
    }

    pub(crate) fn write(file: ResourceRef, data: Vec<u8>, position: i64) -> Self {
        Self {
            kind: Kind::Write {
                file: Some(file),
                data,
                position,
            },
        }
    }

    pub(crate) fn read_console(file: ResourceRef, len: u32) -> Self {
        Self {
            kind: Kind::ReadConsole {
                file: Some(file),
                len,
                result: None,
            },
        }
    }

    pub(crate) fn write_console(file: ResourceRef, data: Vec<u16>) -> Self {
        Self {
            kind: Kind::WriteConsole {
                file: Some(file),
                data,
            },
        }
    }

    pub(crate) fn tty_read_console_input(input: ResourceRef, len: u32) -> AsyncHostResult<Self> {
        Ok(Self {
            kind: Kind::TtyReadConsoleInput {
                input: Some(input),
                #[cfg(windows)]
                cancel: std::sync::Arc::new(tty::make_cancel_event()?),
                len,
                result: None,
                resized: false,
            },
        })
    }

    pub(crate) fn file_kind_by_path(
        parent: Option<ResourceRef>,
        path: OsString,
        follow_symlink: bool,
    ) -> Self {
        Self {
            kind: Kind::FileKindByPath {
                parent,
                path,
                follow_symlink,
            },
        }
    }

    pub(crate) fn file_size(file: ResourceRef) -> Self {
        Self {
            kind: Kind::FileSize {
                file: Some(file),
                result: 0,
            },
        }
    }

    pub(crate) fn file_time(file: ResourceRef) -> Self {
        Self {
            kind: Kind::FileTime {
                file: Some(file),
                result: None,
            },
        }
    }

    pub(crate) fn file_time_by_path(path: OsString, follow_symlink: bool) -> Self {
        Self {
            kind: Kind::FileTimeByPath {
                path,
                follow_symlink,
                result: None,
            },
        }
    }

    pub(crate) fn access(path: OsString, access: i32) -> Self {
        Self {
            kind: Kind::Access { path, access },
        }
    }

    pub(crate) fn chmod(path: OsString, mode: i32) -> Self {
        Self {
            kind: Kind::Chmod { path, mode },
        }
    }

    pub(crate) fn fsync(file: ResourceRef, only_data: bool) -> Self {
        Self {
            kind: Kind::Fsync {
                file: Some(file),
                only_data,
            },
        }
    }

    pub(crate) fn flock(file: ResourceRef, exclusive: bool) -> Self {
        Self {
            kind: Kind::Flock {
                file: Some(file),
                exclusive,
            },
        }
    }

    pub(crate) fn remove(path: OsString) -> Self {
        Self {
            kind: Kind::Remove { path },
        }
    }

    pub(crate) fn rename(old_path: OsString, new_path: OsString, replace: bool) -> Self {
        Self {
            kind: Kind::Rename {
                old_path,
                new_path,
                replace,
            },
        }
    }

    pub(crate) fn symlink(target: OsString, path: OsString, force_symlink: bool) -> Self {
        Self {
            kind: Kind::Symlink {
                target,
                path,
                force_symlink,
            },
        }
    }

    pub(crate) fn mkdir(path: OsString, mode: i32) -> Self {
        Self {
            kind: Kind::Mkdir { path, mode },
        }
    }

    pub(crate) fn rmdir(path: OsString) -> Self {
        Self {
            kind: Kind::Rmdir { path },
        }
    }

    pub(crate) fn readdir(dir: ResourceRef, buffer: CBufferLease, len: u32, restart: bool) -> Self {
        Self {
            kind: Kind::Readdir {
                dir: Some(dir),
                buffer: Some(buffer),
                len,
                restart,
            },
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn inotify_add_watch(inotify: ResourceRef, path: OsString, is_dir: bool) -> Self {
        Self {
            kind: Kind::InotifyAddWatch {
                inotify: Some(inotify),
                path,
                is_dir,
            },
        }
    }

    pub(crate) fn realpath(path: OsString) -> Self {
        Self {
            kind: Kind::Realpath { path, result: None },
        }
    }

    pub(crate) fn check_policy(&self, filesystem: &HostFs) -> AsyncHostResult<()> {
        match &self.kind {
            Kind::Open {
                filename,
                access,
                create_mode,
                append,
                request,
                ..
            } => {
                filesystem.authorize_open(filename, *access, *create_mode, *append)?;
                if request.mask() & !STAT_OPEN_IDENTITY != 0 {
                    filesystem.authorize_metadata(filename)?;
                }
                Ok(())
            }
            Kind::Fstatx { file, .. } => check_file_metadata_policy(filesystem, file.as_deref()),
            Kind::Statx {
                parent,
                path,
                follow_symlink,
                ..
            }
            | Kind::FileKindByPath {
                parent,
                path,
                follow_symlink,
            } => check_path_metadata_policy(filesystem, parent.as_deref(), path, *follow_symlink),
            Kind::FileSize { file, .. } | Kind::FileTime { file, .. } => {
                check_file_metadata_policy(filesystem, file.as_deref())
            }
            Kind::FileTimeByPath {
                path,
                follow_symlink,
                ..
            } => check_path_metadata_policy(filesystem, None, path, *follow_symlink),
            Kind::Realpath { path, .. } => filesystem.authorize_metadata(path),
            #[cfg(target_os = "linux")]
            Kind::InotifyAddWatch { path, .. } => filesystem.authorize_metadata(path),
            Kind::Access { path, access } => filesystem.authorize_access(path, *access),
            Kind::Chmod { path, .. } => filesystem.authorize_write(path),
            Kind::Flock { file, exclusive } => {
                check_file_lock_policy(filesystem, file.as_deref(), *exclusive)
            }
            Kind::Remove { path } => filesystem.authorize_remove(path),
            Kind::Rename {
                old_path, new_path, ..
            } => filesystem.authorize_rename(old_path, new_path),
            Kind::Symlink { path, .. } | Kind::Mkdir { path, .. } | Kind::Rmdir { path } => {
                filesystem.authorize_write_entry(path)
            }
            Kind::Read { .. }
            | Kind::Write { .. }
            | Kind::ReadConsole { .. }
            | Kind::WriteConsole { .. }
            | Kind::TtyReadConsoleInput { .. }
            | Kind::Fsync { .. }
            | Kind::Readdir { .. } => Ok(()),
        }
    }

    pub(crate) fn run(&mut self) -> AsyncHostResult<i64> {
        match &mut self.kind {
            Kind::Open {
                filename,
                access,
                create_mode,
                append,
                sync,
                mode,
                request,
                result,
            } => runner::run_open_job(
                result,
                std::mem::take(filename),
                *access,
                *create_mode,
                *append,
                *sync,
                *mode,
                *request,
            ),
            Kind::Fstatx {
                file,
                request,
                result,
            } => match file.take() {
                Some(file) => stat::run_fstatx_job(&file, *request, result),
                None => Err(AsyncHostError::Badf),
            },
            Kind::Statx {
                parent,
                path,
                request,
                follow_symlink,
                result,
            } => {
                let parent = parent.take();
                stat::run_statx_job(
                    parent.as_deref(),
                    std::mem::take(path),
                    *request,
                    *follow_symlink,
                    result,
                )
            }
            Kind::Read {
                file,
                len,
                position,
                result,
            } => match file.take() {
                Some(file) => runner::run_read_job(&file, *len, *position, result),
                None => Err(AsyncHostError::Badf),
            },
            Kind::Write {
                file,
                data,
                position,
            } => match file.take() {
                Some(file) => runner::run_write_job(&file, data, *position),
                None => Err(AsyncHostError::Badf),
            },
            Kind::ReadConsole { file, len, result } => match file.take() {
                Some(file) => console::read(&file, *len, result),
                None => Err(AsyncHostError::Badf),
            },
            Kind::WriteConsole { file, data } => match file.take() {
                Some(file) => console::write(&file, data),
                None => Err(AsyncHostError::Badf),
            },
            Kind::TtyReadConsoleInput {
                input,
                #[cfg(windows)]
                cancel,
                len,
                result,
                resized,
            } => match input.take() {
                Some(input) => tty::read_console_input(
                    &input,
                    #[cfg(windows)]
                    cancel,
                    *len,
                    result,
                    resized,
                ),
                None => Err(AsyncHostError::Badf),
            },
            Kind::FileKindByPath {
                parent,
                path,
                follow_symlink,
            } => {
                let parent = parent.take();
                runner::run_file_kind_by_path_job(
                    parent.as_deref(),
                    std::mem::take(path),
                    *follow_symlink,
                )
            }
            Kind::FileSize { file, result } => match file.take() {
                Some(file) => runner::run_file_size_job(&file, result),
                None => Err(AsyncHostError::Badf),
            },
            Kind::FileTime { file, result } => match file.take() {
                Some(file) => runner::run_file_time_job(&file, result),
                None => Err(AsyncHostError::Badf),
            },
            Kind::FileTimeByPath {
                path,
                follow_symlink,
                result,
            } => runner::run_file_time_by_path_job(std::mem::take(path), *follow_symlink, result),
            Kind::Access { path, access } => runner::run_access_job(std::mem::take(path), *access),
            Kind::Chmod { path, mode } => runner::run_chmod_job(std::mem::take(path), *mode),
            Kind::Fsync { file, only_data } => match file.take() {
                Some(file) => runner::run_fsync_job(&file, *only_data),
                None => Err(AsyncHostError::Badf),
            },
            Kind::Flock { file, exclusive } => match file.take() {
                Some(file) => runner::run_flock_job(&file, *exclusive),
                None => Err(AsyncHostError::Badf),
            },
            Kind::Remove { path } => runner::run_remove_job(std::mem::take(path)),
            Kind::Rename {
                old_path,
                new_path,
                replace,
            } => {
                runner::run_rename_job(std::mem::take(old_path), std::mem::take(new_path), *replace)
            }
            Kind::Symlink {
                target,
                path,
                force_symlink,
            } => runner::run_symlink_job(
                std::mem::take(target),
                std::mem::take(path),
                *force_symlink,
            ),
            Kind::Mkdir { path, mode } => runner::run_mkdir_job(std::mem::take(path), *mode),
            Kind::Rmdir { path } => runner::run_rmdir_job(std::mem::take(path)),
            Kind::Readdir {
                dir,
                buffer,
                len,
                restart,
            } => match (dir.take(), buffer.as_mut()) {
                (Some(dir), Some(buffer)) => {
                    runner::run_readdir_job(&dir, buffer.as_mut_slice(), *len, *restart)
                }
                _ => Err(AsyncHostError::Badf),
            },
            #[cfg(target_os = "linux")]
            Kind::InotifyAddWatch {
                inotify,
                path,
                is_dir,
            } => match inotify.take() {
                Some(inotify) => {
                    runner::run_inotify_add_watch_job(&inotify, std::mem::take(path), *is_dir)
                }
                None => Err(AsyncHostError::Badf),
            },
            Kind::Realpath { path, result } => {
                runner::run_realpath_job(std::mem::take(path), result)
            }
        }
    }

    #[cfg(windows)]
    pub(crate) fn cancellation_resource(&self) -> Option<ResourceRef> {
        match &self.kind {
            Kind::TtyReadConsoleInput { cancel, .. } => Some(cancel.clone()),
            _ => None,
        }
    }

    pub(crate) fn tty_read_console_input_resized(&self) -> AsyncHostResult<bool> {
        match &self.kind {
            Kind::TtyReadConsoleInput { resized, .. } => Ok(*resized),
            _ => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn take_c_buffer_lease(&mut self) -> Option<CBufferLease> {
        match &mut self.kind {
            Kind::Readdir { buffer, .. } => buffer.take(),
            _ => None,
        }
    }

    pub(crate) fn published_realpath_handle(&self) -> Option<u64> {
        match &self.kind {
            Kind::Realpath {
                result: Some(RealpathJobResult::Published(handle)),
                ..
            } => Some(*handle),
            _ => None,
        }
    }

    pub(crate) fn open_result(&self) -> AsyncHostResult<&OpenJobResult> {
        match &self.kind {
            Kind::Open {
                result: Some(result),
                ..
            } => Ok(result),
            Kind::Open { .. } => Err(AsyncHostError::Inval),
            _ => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn open_result_mut(&mut self) -> AsyncHostResult<&mut OpenJobResult> {
        match &mut self.kind {
            Kind::Open {
                result: Some(result),
                ..
            } => Ok(result),
            Kind::Open { .. } => Err(AsyncHostError::Inval),
            _ => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn stat_result(&self) -> AsyncHostResult<&PackedStat> {
        match &self.kind {
            Kind::Open {
                result: Some(OpenJobResult { stat: result, .. }),
                ..
            }
            | Kind::Fstatx {
                result: Some(result),
                ..
            }
            | Kind::Statx {
                result: Some(result),
                ..
            } => Ok(result),
            Kind::Open { .. } | Kind::Fstatx { .. } | Kind::Statx { .. } => {
                Err(AsyncHostError::Inval)
            }
            _ => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn file_size_result(&self) -> AsyncHostResult<i64> {
        match &self.kind {
            Kind::FileSize { result, .. } => Ok(*result),
            _ => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn publish_realpath_result(
        &mut self,
        publish: impl FnOnce(Box<[u8]>) -> u64,
    ) -> AsyncHostResult<u64> {
        match &mut self.kind {
            Kind::Realpath {
                result: Some(RealpathJobResult::Published(handle)),
                ..
            } => Ok(*handle),
            Kind::Realpath { result, .. } => {
                let Some(RealpathJobResult::Unpublished(buffer)) = result.take() else {
                    return Err(AsyncHostError::Inval);
                };
                let handle = publish(buffer);
                *result = Some(RealpathJobResult::Published(handle));
                Ok(handle)
            }
            _ => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn copy_read_result(
        &self,
        job_error: i32,
        memory: &mut (impl GuestMemory + ?Sized),
        dst: u32,
        offset: u32,
        len: u32,
    ) -> AsyncHostResult<()> {
        if job_error != 0 {
            return Ok(());
        }
        let Kind::Read {
            result: Some(result),
            ..
        } = &self.kind
        else {
            return Err(AsyncHostError::Badf);
        };
        let dst = dst.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        memory.write_with_capacity(dst, len, result)?;
        Ok(())
    }

    // Console offsets and capacities are UTF-16 code units, not bytes.
    // Copy only after completion, into a newly acquired guest memory view.
    pub(crate) fn copy_read_console_result(
        &self,
        job_error: i32,
        memory: &mut (impl GuestMemory + ?Sized),
        dst: u32,
        offset: u32,
        len: u32,
    ) -> AsyncHostResult<()> {
        if job_error != 0 {
            return Ok(());
        }
        let (Kind::ReadConsole {
            result: Some(result),
            ..
        }
        | Kind::TtyReadConsoleInput {
            result: Some(result),
            ..
        }) = &self.kind
        else {
            return Err(AsyncHostError::Badf);
        };
        if result.len() > len as usize {
            return Err(AsyncHostError::Fault);
        }
        let offset = offset.checked_mul(2).ok_or(AsyncHostError::Fault)?;
        let dst = dst.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        let len = len.checked_mul(2).ok_or(AsyncHostError::Fault)?;
        let out = memory.read_exact_mut(dst, len)?;
        for (unit, bytes) in result.iter().zip(out.chunks_exact_mut(2)) {
            bytes.copy_from_slice(&unit.to_le_bytes());
        }
        Ok(())
    }

    pub(crate) fn copy_file_time_result(
        &self,
        job_error: i32,
        memory: &mut (impl GuestMemory + ?Sized),
        dst: u32,
    ) -> AsyncHostResult<()> {
        if job_error != 0 {
            return Ok(());
        }
        let result = match &self.kind {
            Kind::FileTime {
                result: Some(result),
                ..
            }
            | Kind::FileTimeByPath {
                result: Some(result),
                ..
            } => result,
            _ => return Err(AsyncHostError::Badf),
        };

        let file_time = result.as_native();
        let mut record = [0; 48];
        record[0..8].copy_from_slice(&fd_util::stub::get_atime_sec(file_time).to_le_bytes());
        record[8..12].copy_from_slice(&fd_util::stub::get_atime_nsec(file_time).to_le_bytes());
        record[16..24].copy_from_slice(&fd_util::stub::get_mtime_sec(file_time).to_le_bytes());
        record[24..28].copy_from_slice(&fd_util::stub::get_mtime_nsec(file_time).to_le_bytes());
        record[32..40].copy_from_slice(&fd_util::stub::get_ctime_sec(file_time).to_le_bytes());
        record[40..44].copy_from_slice(&fd_util::stub::get_ctime_nsec(file_time).to_le_bytes());
        memory.write_exact(dst, &record)?;
        Ok(())
    }

    pub(crate) fn copy_stat_result(
        &self,
        job_error: i32,
        memory: &mut (impl GuestMemory + ?Sized),
        dst: u32,
        dst_len: u32,
    ) -> AsyncHostResult<()> {
        if job_error != 0 {
            return Ok(());
        }
        memory.write_with_capacity(dst, dst_len, self.stat_result()?.as_bytes())?;
        Ok(())
    }

    #[cfg(all(test, unix))]
    pub(crate) fn set_read_result(&mut self, bytes: Vec<u8>) -> AsyncHostResult<()> {
        match &mut self.kind {
            Kind::Read { result, .. } => {
                *result = Some(bytes);
                Ok(())
            }
            _ => Err(AsyncHostError::Badf),
        }
    }

    #[cfg(all(test, unix))]
    pub(crate) fn set_realpath_result(&mut self, path: Box<[u8]>) -> AsyncHostResult<()> {
        match &mut self.kind {
            Kind::Realpath { result, .. } => {
                *result = Some(RealpathJobResult::Unpublished(path));
                Ok(())
            }
            _ => Err(AsyncHostError::Badf),
        }
    }
}

impl OpenJobResult {
    pub(crate) fn file_kind(&self) -> AsyncHostResult<i32> {
        self.stat
            .scalar(STAT_FILE_KIND)
            .map(|value| value as i32)
            .ok_or(AsyncHostError::Inval)
    }

    pub(crate) fn device_id(&self) -> AsyncHostResult<u64> {
        self.stat
            .scalar(STAT_DEVICE_ID)
            .ok_or(AsyncHostError::Inval)
    }

    pub(crate) fn file_id(&self) -> AsyncHostResult<u64> {
        self.stat.scalar(STAT_FILE_ID).ok_or(AsyncHostError::Inval)
    }
}

fn check_file_metadata_policy(filesystem: &HostFs, file: Option<&Resource>) -> AsyncHostResult<()> {
    let file = file.ok_or(AsyncHostError::Badf)?;
    filesystem.authorize_resource_metadata(file.canonical_path())
}

fn check_path_metadata_policy(
    filesystem: &HostFs,
    parent: Option<&Resource>,
    path: &OsStr,
    follow_symlink: bool,
) -> AsyncHostResult<()> {
    match (parent, follow_symlink) {
        (None, true) => filesystem.authorize_metadata(path),
        (None, false) => filesystem.authorize_entry_metadata(path),
        (Some(parent), true) => filesystem.authorize_metadata_at(parent.canonical_path(), path),
        (Some(parent), false) => {
            filesystem.authorize_entry_metadata_at(parent.canonical_path(), path)
        }
    }
}

fn check_file_lock_policy(
    filesystem: &HostFs,
    file: Option<&Resource>,
    exclusive: bool,
) -> AsyncHostResult<()> {
    let file = file.ok_or(AsyncHostError::Badf)?;
    filesystem.authorize_resource_lock(file.canonical_path(), exclusive)
}

#[cfg(test)]
pub(crate) fn ported_symbols() -> Vec<crate::async_sys::PortedSymbol> {
    [runner::PORTED_SYMBOLS, console::PORTED_SYMBOLS].concat()
}

#[cfg(test)]
pub(crate) fn compat_symbols() -> Vec<crate::async_sys::CompatSymbol> {
    runner::COMPAT_SYMBOLS.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_read_result_uses_utf16_units_and_checks_the_entire_destination() {
        let job = Job {
            kind: Kind::ReadConsole {
                file: None,
                len: 3,
                result: Some(vec![0x03bb, 0xd83d, 0xde00]),
            },
        };
        let mut guest = [0xaa; 12];
        job.copy_read_console_result(0, &mut guest, 1, 1, 3)
            .unwrap();
        assert_eq!(&guest[3..9], &[0xbb, 0x03, 0x3d, 0xd8, 0x00, 0xde]);
        assert_eq!(&guest[..3], &[0xaa; 3]);
        assert_eq!(&guest[9..], &[0xaa; 3]);
        for (offset, len) in [(1, 2), (u32::MAX, 3), (0, u32::MAX), (3, 3)] {
            let mut guest = [0xaa; 8];
            assert_eq!(
                job.copy_read_console_result(0, &mut guest, 1, offset, len),
                Err(AsyncHostError::Fault)
            );
            assert_eq!(guest, [0xaa; 8]);
        }
        // A failed/cancelled native read must not expose an uninitialized buffer.
        let failed = Job {
            kind: Kind::ReadConsole {
                file: None,
                len: 3,
                result: None,
            },
        };
        failed
            .copy_read_console_result(1, &mut guest, u32::MAX, u32::MAX, u32::MAX)
            .unwrap();
    }

    #[test]
    fn fixed_cwd_preserves_policy_denial_errors() {
        use crate::policy;
        use crate::runtime::{Env, WorkingDirectory};
        use std::sync::Arc;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("denied.txt");
        std::fs::write(&path, []).unwrap();
        for cwd in [
            WorkingDirectory::Ambient,
            WorkingDirectory::Fixed(directory.path().to_owned()),
        ] {
            let (mut policy, _) =
                policy::load_inherited_json(b"{}", &crate::runtime::Env::ambient()).unwrap();
            let filesystem = HostFs::new(
                policy.take_filesystem_policy(),
                Arc::new(Env::ambient()),
                Arc::new(cwd),
            );
            let path = path.clone().into_os_string();
            for job in [
                Job::access(path.clone(), 0),
                Job::file_kind_by_path(None, path.clone(), true),
                Job::file_kind_by_path(None, path, false),
            ] {
                assert_eq!(
                    job.check_policy(&filesystem),
                    Err(AsyncHostError::PermissionDenied),
                );
            }
        }
    }

    #[test]
    fn empty_rmdir_job_does_not_delete_the_execution_directory() {
        use crate::runtime::{Env, WorkingDirectory};
        use std::sync::Arc;

        let directory = tempfile::tempdir().unwrap();
        let filesystem = HostFs::new(
            None,
            Arc::new(Env::ambient()),
            Arc::new(WorkingDirectory::Fixed(directory.path().to_owned())),
        );
        let mut job = Job::rmdir(OsString::new());
        job.check_policy(&filesystem).unwrap();
        job.configure_paths(&filesystem);

        let result = job.run();
        assert!(directory.path().is_dir());
        assert!(result.is_err());
    }

    #[test]
    fn job_executors_reference_native_worker_symbols() {
        let async_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../third_party/moonbitlang_async");
        for symbol in ported_symbols() {
            let source_path = async_root.join(symbol.source);
            let contents = std::fs::read_to_string(&source_path)
                .unwrap_or_else(|error| panic!("failed to read {:?}: {error}", source_path));
            assert!(
                contents.contains(symbol.native_symbol),
                "{:?} does not contain native worker symbol {} for {}::{}",
                source_path,
                symbol.native_symbol,
                symbol.rust_module,
                symbol.rust_symbol
            );
        }
    }

    #[test]
    fn read_job_carries_resource_and_length_payload() {
        let file = std::sync::Arc::new(Resource::invalid());
        let job = Job::read(std::sync::Arc::clone(&file), 8, -1);

        match &job.kind {
            Kind::Read {
                file: Some(actual_file),
                len,
                position,
                result: None,
            } => {
                assert!(std::sync::Arc::ptr_eq(actual_file, &file));
                assert_eq!(*len, 8);
                assert_eq!(*position, -1);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn open_job_carries_owned_path_and_open_flags() {
        let job = Job::open_legacy(OsString::from("/tmp/example"), 2, 3, true, 1, 0o644);

        match &job.kind {
            Kind::Open {
                filename,
                access,
                create_mode,
                append,
                sync,
                mode,
                request,
                result: None,
            } => {
                assert_eq!(filename, &OsString::from("/tmp/example"));
                assert_eq!(*access, 2);
                assert_eq!(*create_mode, 3);
                assert!(*append);
                assert_eq!(*sync, 1);
                assert_eq!(*mode, 0o644);
                assert_eq!(request.mask(), STAT_OPEN_IDENTITY);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn realpath_job_carries_owned_path() {
        let job = Job::realpath(OsString::from("/tmp/example"));

        match &job.kind {
            Kind::Realpath { path, result: None } => {
                assert_eq!(path, &OsString::from("/tmp/example"));
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn realpath_job_resolves_to_nul_terminated_c_buffer() {
        use std::os::unix::ffi::OsStrExt;

        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        let link = tmp.path().join("link");
        std::fs::create_dir(&target).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let mut job = Job::realpath(link.into_os_string());

        assert_eq!(job.run(), Ok(0));
        let Kind::Realpath {
            result: Some(RealpathJobResult::Unpublished(buffer)),
            ..
        } = &job.kind
        else {
            panic!("expected completed realpath Job");
        };
        let realpath = std::ffi::CStr::from_bytes_with_nul(buffer.as_ref()).unwrap();
        let expected = std::fs::canonicalize(target).unwrap();
        assert_eq!(realpath.to_bytes(), expected.as_os_str().as_bytes());
    }

    #[test]
    fn realpath_result_is_published_once() {
        let mut job = Job::realpath(OsString::from("unused"));
        let Kind::Realpath { result, .. } = &mut job.kind else {
            unreachable!();
        };
        *result = Some(RealpathJobResult::Unpublished(
            b"resolved\0".to_vec().into_boxed_slice(),
        ));

        let handle = job
            .publish_realpath_result(|buffer| {
                assert_eq!(&*buffer, b"resolved\0");
                42
            })
            .unwrap();
        let same_handle = job
            .publish_realpath_result(|_| {
                panic!("a published realpath result must not be published again")
            })
            .unwrap();

        assert_eq!(handle, 42);
        assert_eq!(same_handle, handle);
    }

    #[test]
    fn resource_job_releases_file_when_worker_finishes() {
        let file = std::sync::Arc::new(Resource::invalid());
        let file_ref = std::sync::Arc::downgrade(&file);
        let mut job = Job::flock(std::sync::Arc::clone(&file), true);
        drop(file);

        let _ = job.run();

        assert!(file_ref.upgrade().is_none());
    }
}
