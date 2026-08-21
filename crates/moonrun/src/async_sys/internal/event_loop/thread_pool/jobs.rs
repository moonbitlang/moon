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
#[cfg(any(unix, windows))]
use std::sync::Arc;

use crate::async_host::{AsyncHostError, AsyncHostResult};
#[cfg(unix)]
use crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier;
use crate::async_sys::ported_fns;
use crate::resource::{ResourcePublication, ResourceRef};

use super::types::{HostHandle, Job, JobPayload, SpawnOptions, platform};

pub(crate) fn make_failed_job(errno: i32) -> Job {
    Job::new(JobPayload::Failed { errno })
}

pub(crate) fn get_platform() -> i32 {
    platform()
}

ported_fns! {
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
}

pub(crate) fn make_sleep_job(ms: i32) -> Job {
    Job::new(JobPayload::Sleep { duration_ms: ms })
}

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

pub(crate) fn spawn_job_set_cwd(job: &mut Job, cwd: OsString) -> AsyncHostResult<()> {
    match job.payload_mut() {
        #[cfg(unix)]
        JobPayload::SpawnUnix { cwd: job_cwd, .. } => {
            *job_cwd = Some(cwd);
            Ok(())
        }
        #[cfg(windows)]
        JobPayload::SpawnWindows { cwd: job_cwd, .. } => {
            *job_cwd = Some(cwd);
            Ok(())
        }
        _ => Err(AsyncHostError::Badf),
    }
}

#[cfg(windows)]
pub(crate) fn spawn_job_set_no_console_window(job: &mut Job) -> AsyncHostResult<()> {
    match job.payload_mut() {
        JobPayload::SpawnWindows { options, .. } => {
            options.no_console_window = true;
            Ok(())
        }
        _ => Err(AsyncHostError::Badf),
    }
}

pub(crate) fn get_spawn_job_result_handle(job: &Job) -> AsyncHostResult<HostHandle> {
    match job.payload() {
        #[cfg(unix)]
        JobPayload::SpawnUnix {
            result: Some(ResourcePublication::Published(handle)),
            ..
        } => Ok(*handle),
        #[cfg(windows)]
        JobPayload::SpawnWindows {
            result: Some(ResourcePublication::Published(handle)),
            ..
        } => Ok(*handle),
        #[cfg(unix)]
        JobPayload::SpawnUnix {
            result: Some(ResourcePublication::Unpublished(_)),
            ..
        } => Err(AsyncHostError::Inval),
        #[cfg(windows)]
        JobPayload::SpawnWindows {
            result: Some(ResourcePublication::Unpublished(_)),
            ..
        } => Err(AsyncHostError::Inval),
        #[cfg(unix)]
        JobPayload::SpawnUnix { result: None, .. } => Ok(crate::async_host::INVALID_HOST_HANDLE),
        #[cfg(windows)]
        JobPayload::SpawnWindows { result: None, .. } => Ok(crate::async_host::INVALID_HOST_HANDLE),
        _ => Err(AsyncHostError::Badf),
    }
}

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

#[cfg(unix)]
pub(crate) fn make_sigwait_job(
    signals: Vec<i32>,
    notifier: Arc<ThreadPoolCompletionNotifier>,
) -> Job {
    Job::new(JobPayload::Sigwait { signals, notifier })
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

pub(crate) fn take_spawn_job_result(job: &mut Job) -> AsyncHostResult<Option<ResourcePublication>> {
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
    resource: ResourcePublication,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_sys::internal::event_loop::thread_pool::run_host_job;

    #[test]
    fn sleep_job_initial_result_matches_native_job_header() {
        let job = make_sleep_job(0);

        assert_eq!(job_get_ret(&job), 0);
        assert_eq!(job_get_err(&job), 0);
    }

    #[test]
    fn sleep_job_runs_without_error() {
        let mut job = make_sleep_job(0);

        run_host_job(&mut job);

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
