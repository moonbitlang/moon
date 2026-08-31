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

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::fd_util;
#[cfg(unix)]
use crate::async_sys::signal::SigwaitJob;
use crate::filesystem::Job as FilesystemJob;
use crate::network::Job as NetworkJob;
use crate::process::Job as ProcessJob;

pub(crate) type ResourceHandle = u64;
pub(crate) type HostHandle = ResourceHandle;

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

    pub(crate) fn process(&self) -> AsyncHostResult<&ProcessJob> {
        match &self.payload {
            JobPayload::Process(job) => Ok(job),
            _ => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn process_mut(&mut self) -> AsyncHostResult<&mut ProcessJob> {
        match &mut self.payload {
            JobPayload::Process(job) => Ok(job),
            _ => Err(AsyncHostError::Badf),
        }
    }

    #[cfg(windows)]
    pub(crate) fn cancellation_resource(&self) -> Option<crate::resource::ResourceRef> {
        match &self.payload {
            JobPayload::Process(job) => job.cancellation_resource(),
            _ => None,
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

impl From<ProcessJob> for Job {
    fn from(job: ProcessJob) -> Self {
        Self::new(JobPayload::Process(job))
    }
}

#[cfg(unix)]
impl From<SigwaitJob> for Job {
    fn from(job: SigwaitJob) -> Self {
        Self::new(JobPayload::Signal(job))
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
    Process(ProcessJob),
    #[cfg(unix)]
    Signal(SigwaitJob),
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
