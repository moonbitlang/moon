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

use std::ffi::OsString;

use crate::async_host::{AsyncHostError, AsyncHostResult, types::Platform};
use crate::async_sys::ported_fns;

use super::process::HostProcess;
use super::types::{GuestBuffer, HostHandle, Job, JobPayload, OpenJobResult, platform};

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_get_platform"
    )]
    pub(crate) fn get_platform() -> Platform {
        platform()
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_job_get_ret"
    )]
    #[allow(dead_code)]
    pub(crate) fn job_get_ret(job: &Job) -> i64 {
        job.ret()
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_job_get_err"
    )]
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub(crate) fn make_sleep_job(ms: i32) -> Job {
        Job::new(JobPayload::Sleep { duration_ms: ms })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_read_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_read_job(
        fd: HostHandle,
        ptr: i32,
        offset: i32,
        len: i32,
        position: i64,
    ) -> Job {
        Job::new(JobPayload::Read {
            fd,
            dst: GuestBuffer::new(ptr, offset, len),
            position,
            result: None,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_write_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_write_job(fd: HostHandle, data: Vec<u8>, position: i64) -> Job {
        Job::new(JobPayload::Write { fd, data, position })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_open_job"
    )]
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub(crate) fn make_file_kind_by_path_job(
        parent: HostHandle,
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
    #[allow(dead_code)]
    pub(crate) fn open_job_get_fd(result: &OpenJobResult) -> HostHandle {
        result.fd
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_open_job_get_kind"
    )]
    #[allow(dead_code)]
    pub(crate) fn open_job_get_kind(result: &OpenJobResult) -> i32 {
        result.kind
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_open_job_get_dev_id"
    )]
    #[allow(dead_code)]
    pub(crate) fn open_job_get_dev_id(result: &OpenJobResult) -> u64 {
        result.dev_id
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_open_job_get_file_id"
    )]
    #[allow(dead_code)]
    pub(crate) fn open_job_get_file_id(result: &OpenJobResult) -> u64 {
        result.file_id
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_file_size_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_file_size_job(fd: HostHandle) -> Job {
        Job::new(JobPayload::FileSize { fd, result: 0 })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_file_time_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_file_time_job(fd: HostHandle, ptr: i32, len: i32) -> Job {
        Job::new(JobPayload::FileTime {
            fd,
            out: GuestBuffer::new(ptr, 0, len),
            result: None,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_file_time_by_path_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_file_time_by_path_job(
        path: OsString,
        ptr: i32,
        len: i32,
        follow_symlink: bool,
    ) -> Job {
        Job::new(JobPayload::FileTimeByPath {
            path,
            out: GuestBuffer::new(ptr, 0, len),
            follow_symlink,
            result: None,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_access_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_access_job(path: OsString, access: i32) -> Job {
        Job::new(JobPayload::Access { path, access })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_chmod_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_chmod_job(path: OsString, mode: i32) -> Job {
        Job::new(JobPayload::Chmod { path, mode })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_get_file_size_result"
    )]
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub(crate) fn make_fsync_job(fd: HostHandle, only_data: bool) -> Job {
        Job::new(JobPayload::Fsync { fd, only_data })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_flock_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_flock_job(fd: HostHandle, exclusive: bool) -> Job {
        Job::new(JobPayload::Flock { fd, exclusive })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_remove_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_remove_job(path: OsString) -> Job {
        Job::new(JobPayload::Remove { path })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_rename_job"
    )]
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub(crate) fn make_symlink_job(target: OsString, path: OsString) -> Job {
        Job::new(JobPayload::Symlink { target, path })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_mkdir_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_mkdir_job(path: OsString, mode: i32) -> Job {
        Job::new(JobPayload::Mkdir { path, mode })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_rmdir_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_rmdir_job(path: OsString) -> Job {
        Job::new(JobPayload::Rmdir { path })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_readdir_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_readdir_job(dir: HostHandle, ptr: i32, len: i32, restart: bool) -> Job {
        Job::new(JobPayload::Readdir {
            dir,
            dst: GuestBuffer::new(ptr, 0, len),
            restart,
            result: None,
        })
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_make_wait_for_process_job"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_wait_for_process_job(process: HostProcess) -> Job {
        Job::new(JobPayload::WaitForProcess { process })
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleep_job_initial_result_matches_native_job_header() {
        let job = make_sleep_job(0);

        assert_eq!(job_get_ret(&job), 0);
        assert_eq!(job_get_err(&job), 0);
    }

    #[test]
    fn read_job_carries_host_handle_and_guest_buffer_payload() {
        let job = make_read_job(7, 100, 2, 8, -1);

        assert_eq!(
            job.payload(),
            &JobPayload::Read {
                fd: 7,
                dst: GuestBuffer::new(100, 2, 8),
                position: -1,
                result: None
            }
        );
    }

    #[test]
    fn open_job_carries_owned_path_and_open_flags() {
        let job = make_open_job(OsString::from("/tmp/example"), 2, 3, true, 1, 0o644);

        assert_eq!(
            job.payload(),
            &JobPayload::Open {
                filename: OsString::from("/tmp/example"),
                access: 2,
                create_mode: 3,
                append: true,
                sync: 1,
                mode: 0o644,
                result: None
            }
        );
    }

    #[test]
    fn sleep_job_runs_without_error() {
        let mut job = make_sleep_job(0);

        job.run();

        assert_eq!(job_get_ret(&job), 0);
        assert_eq!(job_get_err(&job), 0);
    }

    #[cfg(unix)]
    #[test]
    fn unix_errno_is_cancelled_matches_async_stub() {
        assert!(errno_is_cancelled(libc::EINTR));
        assert!(!errno_is_cancelled(libc::EINVAL));
    }
}
