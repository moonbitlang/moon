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

use std::ffi::{OsStr, OsString};
#[cfg(any(unix, windows))]
use std::sync::Arc;

use crate::async_host::{AsyncHostError, AsyncHostResult};
#[cfg(unix)]
use crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier;
use crate::async_sys::ported_fns;

use super::types::{
    HostHandle, Job, JobPayload, OpenJobResource, OpenJobResult, ResourceRef, SpawnOptions,
    platform,
};

pub(crate) fn make_failed_job(errno: i32) -> Job {
    Job::new(JobPayload::Failed { errno })
}

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_get_platform"
    )]
    pub(crate) fn get_platform() -> i32 {
        platform()
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_job_get_ret"
    )]
    pub(crate) fn job_get_ret(job: &Job) -> i64 {
        job.ret()
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_job_get_err"
    )]
    pub(crate) fn job_get_err(job: &Job) -> i32 {
        job.err()
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_errno_is_cancelled"
    )]
    pub(crate) fn errno_is_cancelled(errno: i32) -> bool {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::ERROR_OPERATION_ABORTED;
            errno == ERROR_OPERATION_ABORTED as i32
        }
        #[cfg(unix)]
        {
            errno == libc::EINTR
        }
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_sleep_job"
    )]
    pub(crate) fn make_sleep_job(ms: i32) -> Job {
        Job::new(JobPayload::Sleep { duration_ms: ms })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_read_job"
    )]
    pub(crate) fn make_read_job(
        file: ResourceRef,
        len: i32,
        position: i64,
    ) -> Job {
        Job::new(JobPayload::Read {
            file: Some(file),
            len,
            position,
            result: None,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_write_job"
    )]
    pub(crate) fn make_write_job(file: ResourceRef, data: Vec<u8>, position: i64) -> Job {
        Job::new(JobPayload::Write {
            file: Some(file),
            data,
            position,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_open_job"
    )]
    pub(crate) fn make_open_job(
        filename: OsString,
        access: i32,
        create_mode: i32,
        append: bool,
        sync: i32,
        mode: i32,
    ) -> Job {
        Job::new(JobPayload::Open {
            filename,
            access,
            create_mode,
            append,
            sync,
            mode,
            result: None,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_file_kind_by_path_job"
    )]
    pub(crate) fn make_file_kind_by_path_job(
        parent: Option<ResourceRef>,
        path: OsString,
        follow_symlink: bool,
    ) -> Job {
        Job::new(JobPayload::FileKindByPath {
            parent,
            path,
            follow_symlink,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_open_job_get_fd"
    )]
    pub(crate) fn open_job_get_fd(result: &OpenJobResult) -> AsyncHostResult<HostHandle> {
        match &result.resource {
            OpenJobResource::Published(fd) => Ok(*fd),
            OpenJobResource::Unpublished(_) => Err(AsyncHostError::Inval),
        }
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_open_job_get_kind"
    )]
    pub(crate) fn open_job_get_kind(result: &OpenJobResult) -> i32 {
        result.kind
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_open_job_get_dev_id"
    )]
    pub(crate) fn open_job_get_dev_id(result: &OpenJobResult) -> u64 {
        result.dev_id
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_open_job_get_file_id"
    )]
    pub(crate) fn open_job_get_file_id(result: &OpenJobResult) -> u64 {
        result.file_id
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_file_size_job"
    )]
    pub(crate) fn make_file_size_job(file: ResourceRef) -> Job {
        Job::new(JobPayload::FileSize {
            file: Some(file),
            result: 0,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_file_time_job"
    )]
    pub(crate) fn make_file_time_job(file: ResourceRef) -> Job {
        Job::new(JobPayload::FileTime {
            file: Some(file),
            result: None,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_file_time_by_path_job"
    )]
    pub(crate) fn make_file_time_by_path_job(
        path: OsString,
        follow_symlink: bool,
    ) -> Job {
        Job::new(JobPayload::FileTimeByPath {
            path,
            follow_symlink,
            result: None,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_access_job"
    )]
    pub(crate) fn make_access_job(path: OsString, access: i32) -> Job {
        Job::new(JobPayload::Access { path, access })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_chmod_job"
    )]
    pub(crate) fn make_chmod_job(path: OsString, mode: i32) -> Job {
        Job::new(JobPayload::Chmod { path, mode })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_get_file_size_result"
    )]
    pub(crate) fn get_file_size_result(job: &Job) -> AsyncHostResult<i64> {
        match job.payload() {
            JobPayload::FileSize { result, .. } => Ok(*result),
            _ => Err(AsyncHostError::Badf),
        }
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_fsync_job"
    )]
    pub(crate) fn make_fsync_job(file: ResourceRef, only_data: bool) -> Job {
        Job::new(JobPayload::Fsync {
            file: Some(file),
            only_data,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_flock_job"
    )]
    pub(crate) fn make_flock_job(file: ResourceRef, exclusive: bool) -> Job {
        Job::new(JobPayload::Flock {
            file: Some(file),
            exclusive,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_remove_job"
    )]
    pub(crate) fn make_remove_job(path: OsString) -> Job {
        Job::new(JobPayload::Remove { path })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_rename_job"
    )]
    pub(crate) fn make_rename_job(old_path: OsString, new_path: OsString, replace: bool) -> Job {
        Job::new(JobPayload::Rename {
            old_path,
            new_path,
            replace,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_symlink_job"
    )]
    pub(crate) fn make_symlink_job(target: OsString, path: OsString, force_symlink: bool) -> Job {
        Job::new(JobPayload::Symlink {
            target,
            path,
            force_symlink,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_mkdir_job"
    )]
    pub(crate) fn make_mkdir_job(path: OsString, mode: i32) -> Job {
        Job::new(JobPayload::Mkdir { path, mode })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_rmdir_job"
    )]
    pub(crate) fn make_rmdir_job(path: OsString) -> Job {
        Job::new(JobPayload::Rmdir { path })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_readdir_job"
    )]
    pub(crate) fn make_readdir_job(
        dir: ResourceRef,
        buffer: crate::async_host::HostCBuffer,
        len: i32,
        restart: bool,
    ) -> Job {
        Job::new(JobPayload::Readdir {
            dir: Some(dir),
            buffer,
            len,
            restart,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_bind_job"
    )]
    pub(crate) fn make_bind_job(socket: ResourceRef, addr: Vec<u8>) -> Job {
        Job::new(JobPayload::Bind {
            socket: Some(socket),
            addr,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_getaddrinfo_job"
    )]
    pub(crate) fn make_getaddrinfo_job(host: OsString) -> Job {
        Job::new(JobPayload::GetAddrInfo { host, result: None })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_spawn_job"
    )]
    #[allow(clippy::too_many_arguments)]
    #[cfg(unix)]
    pub(crate) fn make_spawn_job_unix(
        path: OsString,
        args: Vec<OsString>,
        env: Vec<OsString>,
        stdin: Option<ResourceRef>,
        stdout: Option<ResourceRef>,
        stderr: Option<ResourceRef>,
        cwd: Option<OsString>,
        options: SpawnOptions,
    ) -> Job {
        Job::new(JobPayload::SpawnUnix {
            path,
            args,
            env,
            options,
            stdio: [stdin, stdout, stderr],
            cwd,
            result: None,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_spawn_job"
    )]
    #[allow(clippy::too_many_arguments)]
    #[cfg(windows)]
    pub(crate) fn make_spawn_job_windows(
        command_line: OsString,
        env: Vec<u16>,
        stdin: Option<ResourceRef>,
        stdout: Option<ResourceRef>,
        stderr: Option<ResourceRef>,
        cwd: Option<OsString>,
        options: SpawnOptions,
    ) -> Job {
        Job::new(JobPayload::SpawnWindows {
            command_line,
            env,
            options,
            stdio: [stdin, stdout, stderr],
            cwd,
            result: None,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_get_spawn_job_result_handle"
    )]
    pub(crate) fn get_spawn_job_result_handle(job: &Job) -> AsyncHostResult<HostHandle> {
        match job.payload() {
            #[cfg(unix)]
            JobPayload::SpawnUnix {
                result: Some(OpenJobResource::Published(handle)),
                ..
            } => Ok(*handle),
            #[cfg(windows)]
            JobPayload::SpawnWindows {
                result: Some(OpenJobResource::Published(handle)),
                ..
            } => Ok(*handle),
            #[cfg(unix)]
            JobPayload::SpawnUnix {
                result: Some(OpenJobResource::Unpublished(_)),
                ..
            } => Err(AsyncHostError::Inval),
            #[cfg(windows)]
            JobPayload::SpawnWindows {
                result: Some(OpenJobResource::Unpublished(_)),
                ..
            } => Err(AsyncHostError::Inval),
            #[cfg(unix)]
            JobPayload::SpawnUnix { result: None, .. } => Ok(crate::async_host::INVALID_HOST_HANDLE),
            #[cfg(windows)]
            JobPayload::SpawnWindows { result: None, .. } => {
                Ok(crate::async_host::INVALID_HOST_HANDLE)
            }
            _ => Err(AsyncHostError::Badf),
        }
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_wait_for_process_job"
    )]
    pub(crate) fn make_wait_for_process_job(
        handle: Option<ResourceRef>,
        tracked_pid: Option<i32>,
        pid: i32,
        #[cfg(unix)] defer_reap: bool,
    ) -> AsyncHostResult<Job> {
        Ok(Job::new(JobPayload::WaitForProcess {
            handle,
            tracked_pid,
            pid,
            #[cfg(unix)]
            defer_reap,
            #[cfg(windows)]
            cancel: Some(Arc::new(super::process::make_wait_for_process_cancel()?)),
        }))
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_sigwait_job"
    )]
    #[cfg(unix)]
    pub(crate) fn make_sigwait_job(
        signals: Vec<i32>,
        notifier: Arc<ThreadPoolCompletionNotifier>,
    ) -> Job {
        Job::new(JobPayload::Sigwait { signals, notifier })
    }
}

#[cfg(windows)]
pub(crate) fn job_cancel_resource(job: &Job) -> Option<ResourceRef> {
    match job.payload() {
        JobPayload::WaitForProcess {
            cancel: Some(cancel),
            ..
        } => Some(Arc::clone(cancel)),
        _ => None,
    }
}

#[cfg(windows)]
pub(crate) fn cancel_job_resource(cancel: &ResourceRef) -> AsyncHostResult<()> {
    super::process::cancel_wait_for_process(cancel)
}

pub(crate) fn open_job_result(job: &Job) -> AsyncHostResult<&OpenJobResult> {
    match job.payload() {
        JobPayload::Open {
            result: Some(result),
            ..
        } => Ok(result),
        JobPayload::Open { .. } => Err(AsyncHostError::Inval),
        _ => Err(AsyncHostError::Badf),
    }
}

pub(crate) fn open_job_result_mut(job: &mut Job) -> AsyncHostResult<&mut OpenJobResult> {
    match job.payload_mut() {
        JobPayload::Open {
            result: Some(result),
            ..
        } => Ok(result),
        JobPayload::Open { .. } => Err(AsyncHostError::Inval),
        _ => Err(AsyncHostError::Badf),
    }
}

pub(crate) fn take_spawn_job_result(job: &mut Job) -> AsyncHostResult<Option<OpenJobResource>> {
    match job.payload_mut() {
        #[cfg(unix)]
        JobPayload::SpawnUnix { result, .. } => Ok(result.take()),
        #[cfg(windows)]
        JobPayload::SpawnWindows { result, .. } => Ok(result.take()),
        _ => Err(AsyncHostError::Badf),
    }
}

pub(crate) fn set_spawn_job_result(
    job: &mut Job,
    resource: OpenJobResource,
) -> AsyncHostResult<()> {
    match job.payload_mut() {
        #[cfg(unix)]
        JobPayload::SpawnUnix { result, .. } => {
            *result = Some(resource);
            Ok(())
        }
        #[cfg(windows)]
        JobPayload::SpawnWindows { result, .. } => {
            *result = Some(resource);
            Ok(())
        }
        _ => Err(AsyncHostError::Badf),
    }
}

pub(crate) fn getaddrinfo_job_result(job: &Job) -> AsyncHostResult<(&OsStr, &[Box<[u8]>])> {
    match job.payload() {
        JobPayload::GetAddrInfo {
            host,
            result: Some(result),
        } => Ok((host.as_os_str(), result)),
        JobPayload::GetAddrInfo { .. } => Err(AsyncHostError::Inval),
        _ => Err(AsyncHostError::Badf),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_sys::internal::event_loop::thread_pool::{Resource, run_host_job};
    use std::sync::Arc;

    #[test]
    fn sleep_job_initial_result_matches_native_job_header() {
        let job = make_sleep_job(0);

        assert_eq!(job_get_ret(&job), 0);
        assert_eq!(job_get_err(&job), 0);
    }

    #[test]
    fn read_job_carries_resource_and_length_payload() {
        let file = Arc::new(Resource::invalid());
        let job = make_read_job(Arc::clone(&file), 8, -1);

        match job.payload() {
            JobPayload::Read {
                file: Some(actual_file),
                len,
                position,
                result: None,
            } => {
                assert!(Arc::ptr_eq(actual_file, &file));
                assert_eq!(*len, 8);
                assert_eq!(*position, -1);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn open_job_carries_owned_path_and_open_flags() {
        let job = make_open_job(OsString::from("/tmp/example"), 2, 3, true, 1, 0o644);

        match job.payload() {
            JobPayload::Open {
                filename,
                access,
                create_mode,
                append,
                sync,
                mode,
                result: None,
            } => {
                assert_eq!(filename, &OsString::from("/tmp/example"));
                assert_eq!(*access, 2);
                assert_eq!(*create_mode, 3);
                assert!(*append);
                assert_eq!(*sync, 1);
                assert_eq!(*mode, 0o644);
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[test]
    fn sleep_job_runs_without_error() {
        let mut job = make_sleep_job(0);

        run_host_job(&mut job);

        assert_eq!(job_get_ret(&job), 0);
        assert_eq!(job_get_err(&job), 0);
    }

    #[test]
    fn resource_job_releases_file_when_worker_finishes() {
        let file = Arc::new(Resource::invalid());
        let file_ref = Arc::downgrade(&file);
        let mut job = make_flock_job(Arc::clone(&file), true);
        drop(file);

        run_host_job(&mut job);

        assert!(file_ref.upgrade().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn unix_errno_is_cancelled_matches_async_stub() {
        assert!(errno_is_cancelled(libc::EINTR));
        assert!(!errno_is_cancelled(libc::EINVAL));
    }
}
