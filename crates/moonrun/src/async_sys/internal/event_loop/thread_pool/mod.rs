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

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs::File;

use crate::async_host::{
    AsyncHostError, AsyncHostResult, GuestMemory, GuestRange, types::Platform,
};
use crate::async_sys::ported_fns;

mod fs;
mod process;
mod sleep;
mod wakeup;

use fs::{
    run_access_job, run_chmod_job, run_file_kind_by_path_job, run_file_size_job,
    run_file_time_by_path_job, run_file_time_job, run_flock_job, run_fsync_job, run_mkdir_job,
    run_open_job, run_read_job, run_readdir_job, run_remove_job, run_rename_job, run_rmdir_job,
    run_symlink_job, run_write_job,
};
pub(crate) use process::HostProcess;
use process::run_wait_for_process_job;
use sleep::run_sleep_job;
use wakeup::{WorkerThreadId, WorkerWakeup, cancel_running_worker};

pub(crate) type HostHandle = i32;

#[derive(Debug)]
pub(crate) struct HostFile {
    file: File,
    pending_dir_entries: VecDeque<Vec<u8>>,
    #[cfg(windows)]
    lock_file: Option<File>,
}

impl HostFile {
    pub(crate) fn new(file: File) -> Self {
        Self {
            file,
            pending_dir_entries: VecDeque::new(),
            #[cfg(windows)]
            lock_file: None,
        }
    }

    pub(crate) fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    pub(crate) fn pending_dir_entries_mut(&mut self) -> &mut VecDeque<Vec<u8>> {
        &mut self.pending_dir_entries
    }

    #[cfg(windows)]
    pub(crate) fn set_lock_file(&mut self, file: File) {
        self.lock_file = Some(file);
    }

    #[cfg(windows)]
    pub(crate) fn take_lock_file(&mut self) -> Option<File> {
        self.lock_file.take()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct GuestBuffer {
    pub(crate) ptr: i32,
    pub(crate) offset: i32,
    pub(crate) len: i32,
}

impl GuestBuffer {
    #[allow(dead_code)]
    pub(crate) fn new(ptr: i32, offset: i32, len: i32) -> Self {
        Self { ptr, offset, len }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct OpenJobResult {
    pub(crate) fd: HostHandle,
    pub(crate) kind: i32,
    pub(crate) dev_id: u64,
    pub(crate) file_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct Job {
    ret: i64,
    err: i32,
    payload: JobPayload,
}

impl Job {
    #[allow(dead_code)]
    fn new(payload: JobPayload) -> Self {
        Self {
            ret: 0,
            err: 0,
            payload,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn run(&mut self) {
        self.ret = 0;
        self.err = 0;

        match &mut self.payload {
            JobPayload::Sleep { duration_ms } => run_sleep_job(*duration_ms),
            _ => self.err = unsupported_job_errno(),
        }
    }

    pub(crate) fn payload(&self) -> &JobPayload {
        &self.payload
    }

    pub(crate) fn payload_mut(&mut self) -> &mut JobPayload {
        &mut self.payload
    }

    pub(crate) fn set_ret(&mut self, ret: i64) {
        self.ret = ret;
        self.err = 0;
    }

    pub(crate) fn set_err(&mut self, err: i32) {
        self.ret = -1;
        self.err = err;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum JobPayload {
    Sleep {
        duration_ms: i32,
    },
    Read {
        fd: HostHandle,
        dst: GuestBuffer,
        position: i64,
        result: Option<Vec<u8>>,
    },
    Write {
        fd: HostHandle,
        data: Vec<u8>,
        position: i64,
    },
    Open {
        filename: OsString,
        access: i32,
        create_mode: i32,
        append: bool,
        sync: i32,
        mode: i32,
        result: Option<OpenJobResult>,
    },
    KindOfFd {
        fd: HostHandle,
    },
    FileKindByPath {
        parent: HostHandle,
        path: OsString,
        follow_symlink: bool,
    },
    FileSize {
        fd: HostHandle,
        result: i64,
    },
    FileTime {
        fd: HostHandle,
        out: GuestBuffer,
        result: Option<Vec<u8>>,
    },
    FileTimeByPath {
        path: OsString,
        out: GuestBuffer,
        follow_symlink: bool,
        result: Option<Vec<u8>>,
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
        fd: HostHandle,
        only_data: bool,
    },
    Flock {
        fd: HostHandle,
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
    },
    Mkdir {
        path: OsString,
        mode: i32,
    },
    Rmdir {
        path: OsString,
    },
    Readdir {
        dir: HostHandle,
        dst: GuestBuffer,
        restart: bool,
        result: Option<Vec<u8>>,
    },
    Realpath {
        path: OsString,
        result: Option<OsString>,
    },
    WaitForProcess {
        process: HostProcess,
    },
    Bind {
        socket: HostHandle,
        addr: Vec<u8>,
    },
    GetAddrInfo {
        hostname: OsString,
    },
    Sigwait {
        signals: Vec<i32>,
    },
    InotifyAddWatch {
        inotify: HostHandle,
        path: OsString,
        is_dir: bool,
    },
}

pub(crate) trait HostFileTable {
    fn insert_file(&mut self, file: File) -> AsyncHostResult<HostHandle>;

    fn with_file_mut<T>(
        &mut self,
        handle: HostHandle,
        f: impl FnOnce(&mut File) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T>;

    fn with_host_file_mut<T>(
        &mut self,
        handle: HostHandle,
        f: impl FnOnce(&mut HostFile) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T>;
}

#[allow(dead_code)]
pub(crate) struct Worker {
    id: Option<WorkerThreadId>,
    job_id: i32,
    job: Option<Job>,
    waiting: bool,
    wakeup: WorkerWakeup,
}

pub(crate) fn run_host_job(job: &mut Job, files: &mut impl HostFileTable) {
    job.set_ret(0);

    let result = match job.payload_mut() {
        JobPayload::Sleep { duration_ms } => {
            run_sleep_job(*duration_ms);
            Ok(0)
        }
        JobPayload::Open {
            filename,
            access,
            create_mode,
            append,
            sync,
            mode,
            result,
        } => run_open_job(
            files,
            result,
            filename.clone(),
            *access,
            *create_mode,
            *append,
            *sync,
            *mode,
        ),
        JobPayload::Read {
            fd,
            dst,
            position,
            result,
        } => run_read_job(files, *fd, *dst, *position, result),
        JobPayload::Write { fd, data, position } => run_write_job(files, *fd, data, *position),
        JobPayload::FileKindByPath {
            parent,
            path,
            follow_symlink,
        } => run_file_kind_by_path_job(files, *parent, path.clone(), *follow_symlink),
        JobPayload::FileSize { fd, result } => run_file_size_job(files, *fd, result),
        JobPayload::FileTime { fd, result, .. } => run_file_time_job(files, *fd, result),
        JobPayload::FileTimeByPath {
            path,
            follow_symlink,
            result,
            ..
        } => run_file_time_by_path_job(path.clone(), *follow_symlink, result),
        JobPayload::Access { path, access } => run_access_job(path.clone(), *access),
        JobPayload::Chmod { path, mode } => run_chmod_job(path.clone(), *mode),
        JobPayload::Fsync { fd, only_data } => run_fsync_job(files, *fd, *only_data),
        JobPayload::Flock { fd, exclusive } => run_flock_job(files, *fd, *exclusive),
        JobPayload::Remove { path } => run_remove_job(path.clone()),
        JobPayload::Rename {
            old_path,
            new_path,
            replace,
        } => run_rename_job(old_path.clone(), new_path.clone(), *replace),
        JobPayload::Symlink { target, path } => run_symlink_job(target.clone(), path.clone()),
        JobPayload::Mkdir { path, mode } => run_mkdir_job(path.clone(), *mode),
        JobPayload::Rmdir { path } => run_rmdir_job(path.clone()),
        JobPayload::Readdir {
            dir,
            dst,
            restart,
            result,
        } => run_readdir_job(files, *dir, *dst, *restart, result),
        JobPayload::WaitForProcess { process } => run_wait_for_process_job(process),
        _ => Err(AsyncHostError::NotSupported),
    };

    match result {
        Ok(ret) => job.set_ret(ret),
        Err(error) => job.set_err(error.errno()),
    }
}

pub(crate) fn complete_guest_job(
    job: &mut Job,
    memory: &mut (impl GuestMemory + ?Sized),
) -> AsyncHostResult<()> {
    if let JobPayload::Read {
        dst,
        result: Some(result),
        ..
    }
    | JobPayload::FileTime {
        out: dst,
        result: Some(result),
        ..
    }
    | JobPayload::FileTimeByPath {
        out: dst,
        result: Some(result),
        ..
    }
    | JobPayload::Readdir {
        dst,
        result: Some(result),
        ..
    } = job.payload_mut()
    {
        let len = i32::try_from(result.len()).map_err(|_| AsyncHostError::Fault)?;
        let dst_offset = dst
            .ptr
            .checked_add(dst.offset)
            .ok_or(AsyncHostError::Fault)?;
        memory.write(GuestRange::new(dst_offset, len)?, result)?;
    }
    Ok(())
}

impl Worker {
    #[allow(dead_code)]
    pub(crate) fn new(init_job_id: i32, init_job: Job) -> Self {
        Self {
            id: None,
            job_id: init_job_id,
            job: Some(init_job),
            waiting: false,
            wakeup: WorkerWakeup::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn wake(&mut self, job_id: i32, job: Option<Job>) {
        self.job_id = job_id;
        self.job = job;
        self.wakeup.wake(self.id, &mut self.waiting);
    }

    #[allow(dead_code)]
    pub(crate) fn enter_idle(&mut self) {
        self.job = None;
    }

    #[allow(dead_code)]
    pub(crate) fn mark_waiting(&mut self) {
        self.waiting = true;
    }

    #[allow(dead_code)]
    pub(crate) fn wait_for_wake(&mut self) {
        self.wakeup.wait(&mut self.waiting);
    }

    #[allow(dead_code)]
    pub(crate) fn cancel(&self) -> i32 {
        if self.waiting {
            return 1;
        }
        cancel_running_worker(self.id)
    }
}

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_get_platform"
    )]
    pub(crate) fn get_platform() -> Platform {
        #[cfg(windows)]
        {
            Platform::Windows
        }
        #[cfg(all(unix, target_os = "macos"))]
        {
            Platform::MacOS
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            Platform::Linux
        }
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_job_get_ret"
    )]
    #[allow(dead_code)]
    pub(crate) fn job_get_ret(job: &Job) -> i64 {
        job.ret
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_job_get_err"
    )]
    #[allow(dead_code)]
    pub(crate) fn job_get_err(job: &Job) -> i32 {
        job.err
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
        original = "moonbitlang_async_spawn_worker"
    )]
    #[allow(dead_code)]
    pub(crate) fn spawn_worker(init_job_id: i32, init_job: Job) -> Worker {
        Worker::new(init_job_id, init_job)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_wake_worker"
    )]
    #[allow(dead_code)]
    pub(crate) fn wake_worker(worker: &mut Worker, job_id: i32, job: Job) {
        worker.wake(job_id, Some(job));
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_worker_enter_idle"
    )]
    #[allow(dead_code)]
    pub(crate) fn worker_enter_idle(worker: &mut Worker) {
        worker.enter_idle();
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_cancel_worker"
    )]
    #[allow(dead_code)]
    pub(crate) fn cancel_worker(worker: &Worker) -> i32 {
        worker.cancel()
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_free_worker"
    )]
    #[allow(dead_code)]
    pub(crate) fn free_worker(_worker: Worker) {}

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_fetch_completion"
    )]
    #[allow(dead_code)]
    pub(crate) fn fetch_completion(
        completions: &mut VecDeque<i32>,
        output: &mut [i32],
    ) -> AsyncHostResult<i32> {
        let n = output.len().min(completions.len());
        for slot in &mut output[..n] {
            *slot = completions.pop_front().ok_or(AsyncHostError::Inval)?;
        }
        i32::try_from(n * std::mem::size_of::<i32>()).map_err(|_| AsyncHostError::Fault)
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

fn unsupported_job_errno() -> i32 {
    #[cfg(unix)]
    {
        libc::ENOSYS
    }
    #[cfg(windows)]
    {
        windows_sys::Win32::Foundation::ERROR_CALL_NOT_IMPLEMENTED as i32
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
            job.payload,
            JobPayload::Read {
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
            job.payload,
            JobPayload::Open {
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

    #[test]
    fn worker_wake_replaces_job_and_leaves_waiting_state() {
        let mut worker = Worker::new(1, make_sleep_job(0));
        worker.mark_waiting();

        worker.wake(2, Some(make_sleep_job(0)));

        assert_eq!(worker.job_id, 2);
        assert!(worker.job.is_some());
        assert!(!worker.waiting);
    }

    #[test]
    fn worker_enter_idle_clears_current_job() {
        let mut worker = Worker::new(1, make_sleep_job(0));

        worker.enter_idle();

        assert!(worker.job.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn unix_errno_is_cancelled_matches_async_stub() {
        assert!(errno_is_cancelled(libc::EINTR));
        assert!(!errno_is_cancelled(libc::EINVAL));
    }
}
