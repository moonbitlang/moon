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
#[cfg(unix)]
use std::sync::Arc;

use crate::async_host::{AsyncHostError, AsyncHostResult};
#[cfg(unix)]
use crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier;
use crate::async_sys::internal::fd_util;
use crate::filesystem::Job as FilesystemJob;
use crate::network::Job as NetworkJob;
use crate::resource::{ResourcePublication, ResourceRef};

pub(crate) type ResourceHandle = u64;
pub(crate) type HostHandle = ResourceHandle;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SpawnOptions {
    #[cfg(unix)]
    pub(crate) child_signal_mask: libc::sigset_t,
    #[cfg(windows)]
    pub(crate) no_console_window: bool,
    #[cfg(windows)]
    pub(crate) is_orphan: bool,
}

/// A host operation following the `moonbitlang/async` native Job contract.
///
/// `err` is reserved for host and system errors handled uniformly by the
/// MoonBit worker loop. When `err` is zero, `ret` is defined by the payload: it
/// may be a value, a success sentinel, or a domain-specific status code.
/// Structured results and domain-specific diagnostics remain in the payload.
#[derive(Debug)]
pub(crate) struct Job {
    ret: i64,
    err: i32,
    payload: JobPayload,
}

impl Job {
    pub(super) fn new(payload: JobPayload) -> Self {
        Self {
            ret: 0,
            err: 0,
            payload,
        }
    }

    pub(crate) fn payload(&self) -> &JobPayload {
        &self.payload
    }

    pub(crate) fn payload_mut(&mut self) -> &mut JobPayload {
        &mut self.payload
    }

    pub(crate) fn filesystem(&self) -> AsyncHostResult<&FilesystemJob> {
        match &self.payload {
            JobPayload::Filesystem(job) => Ok(job),
            _ => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn filesystem_mut(&mut self) -> AsyncHostResult<&mut FilesystemJob> {
        match &mut self.payload {
            JobPayload::Filesystem(job) => Ok(job),
            _ => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn ret(&self) -> i64 {
        self.ret
    }

    pub(crate) fn err(&self) -> i32 {
        self.err
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

impl From<NetworkJob> for Job {
    fn from(job: NetworkJob) -> Self {
        Self::new(JobPayload::Network(job))
    }
}

impl From<FilesystemJob> for Job {
    fn from(job: FilesystemJob) -> Self {
        Self::new(JobPayload::Filesystem(job))
    }
}

#[derive(Debug)]
pub(crate) enum JobPayload {
    Failed {
        errno: i32,
    },
    Sleep {
        duration_ms: i32,
    },
    Filesystem(FilesystemJob),
    Network(NetworkJob),
    #[cfg(unix)]
    SpawnUnix {
        path: OsString,
        args: Vec<OsString>,
        env: Vec<OsString>,
        options: SpawnOptions,
        stdio: [Option<ResourceRef>; 3],
        cwd: Option<OsString>,
        result: Option<ResourcePublication>,
    },
    #[cfg(windows)]
    SpawnWindows {
        command_line: OsString,
        env: Vec<u16>,
        options: SpawnOptions,
        stdio: [Option<ResourceRef>; 3],
        cwd: Option<OsString>,
        result: Option<ResourcePublication>,
    },
    WaitForProcess {
        handle: Option<ResourceRef>,
        // Host-derived identity for policy checks; never supplied by the guest.
        tracked_pid: Option<i32>,
        pid: i32,
        #[cfg(unix)]
        defer_reap: bool,
        #[cfg(windows)]
        cancel: Option<ResourceRef>,
    },
    #[cfg(unix)]
    Sigwait {
        signals: Vec<i32>,
        notifier: Arc<ThreadPoolCompletionNotifier>,
    },
}

pub(crate) trait ResourceTable {
    fn insert_file(&mut self, file: fd_util::stub::RawFd) -> AsyncHostResult<HostHandle>;
}

pub(crate) fn platform() -> i32 {
    #[cfg(windows)]
    {
        2
    }
    #[cfg(target_os = "macos")]
    {
        1
    }
    #[cfg(target_os = "linux")]
    {
        0
    }
}
