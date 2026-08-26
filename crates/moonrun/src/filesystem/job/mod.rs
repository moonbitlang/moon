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

//! Filesystem-owned Job state submitted to moonrun's shared thread pool.

mod runner;
mod stat;

use std::ffi::{OsStr, OsString};

use crate::async_host::{AsyncHostError, AsyncHostResult, CBufferLease};
use crate::async_sys::internal::fd_util;
use crate::guest_memory::GuestMemory;
use crate::policy::Policy;
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

    pub(crate) fn check_policy(&self, policy: &Policy) -> AsyncHostResult<()> {
        match &self.kind {
            Kind::Open {
                filename,
                access,
                create_mode,
                append,
                request,
                ..
            } => {
                policy.open_path(filename, *access, *create_mode, *append)?;
                if request.mask() & !STAT_OPEN_IDENTITY != 0 {
                    policy.stat_path(filename)?;
                }
                Ok(())
            }
            Kind::Fstatx { file, .. } => check_file_metadata_policy(policy, file.as_deref()),
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
            } => check_path_metadata_policy(policy, parent.as_deref(), path, *follow_symlink),
            Kind::FileSize { file, .. } | Kind::FileTime { file, .. } => {
                check_file_metadata_policy(policy, file.as_deref())
            }
            Kind::FileTimeByPath {
                path,
                follow_symlink,
                ..
            } => check_path_metadata_policy(policy, None, path, *follow_symlink),
            Kind::Realpath { path, .. } => policy.stat_path(path),
            #[cfg(target_os = "linux")]
            Kind::InotifyAddWatch { path, .. } => policy.stat_path(path),
            Kind::Access { path, access } => policy.access_path(path, *access),
            Kind::Chmod { path, .. } => policy.chmod_path(path),
            Kind::Flock { file, exclusive } => {
                check_file_lock_policy(policy, file.as_deref(), *exclusive)
            }
            Kind::Remove { path } => policy.remove_path(path),
            Kind::Rename {
                old_path, new_path, ..
            } => policy.rename_path(old_path, new_path),
            Kind::Symlink { path, .. } => policy.symlink_path(path),
            Kind::Mkdir { path, .. } => policy.mkdir_path(path),
            Kind::Rmdir { path } => policy.rmdir_path(path),
            Kind::Read { .. } | Kind::Write { .. } | Kind::Fsync { .. } | Kind::Readdir { .. } => {
                Ok(())
            }
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
    pub(crate) fn published_resource_handle(&self) -> AsyncHostResult<u64> {
        match &self.resource {
            ResourcePublication::Published(handle) => Ok(*handle),
            ResourcePublication::Unpublished(_) => Err(AsyncHostError::Inval),
        }
    }

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

fn check_file_metadata_policy(policy: &Policy, file: Option<&Resource>) -> AsyncHostResult<()> {
    let file = file.ok_or(AsyncHostError::Badf)?;
    policy.stat_resource_path(file.policy_path())
}

fn check_path_metadata_policy(
    policy: &Policy,
    parent: Option<&Resource>,
    path: &OsStr,
    follow_symlink: bool,
) -> AsyncHostResult<()> {
    match (parent, follow_symlink) {
        (None, true) => policy.stat_path(path),
        (None, false) => policy.stat_entry_path(path),
        (Some(parent), true) => policy.stat_path_at(parent.policy_path(), path),
        (Some(parent), false) => policy.stat_entry_path_at(parent.policy_path(), path),
    }
}

fn check_file_lock_policy(
    policy: &Policy,
    file: Option<&Resource>,
    exclusive: bool,
) -> AsyncHostResult<()> {
    let file = file.ok_or(AsyncHostError::Badf)?;
    policy.lock_resource_path(file.policy_path(), exclusive)
}

#[cfg(test)]
pub(crate) fn ported_symbols() -> Vec<crate::async_sys::PortedSymbol> {
    runner::PORTED_SYMBOLS.to_vec()
}

#[cfg(test)]
pub(crate) fn compat_symbols() -> Vec<crate::async_sys::CompatSymbol> {
    runner::COMPAT_SYMBOLS.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

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
