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
use std::sync::Arc;

use crate::async_host::{AsyncHostResult, CBufferLease};
#[cfg(unix)]
use crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier;
use crate::async_sys::internal::fd_util;
use crate::resource::{Resource, ResourceRef};

use super::stat::{PackedStat, StatRequest};

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

#[derive(Debug)]
pub(crate) struct OpenJobResult {
    pub(crate) resource: OpenJobResource,
    pub(super) stat: PackedStat,
}

#[derive(Debug)]
pub(crate) enum OpenJobResource {
    Unpublished(Resource),
    Published(HostHandle),
}

#[derive(Clone, Copy)]
pub(crate) struct FileTimeResult(fd_util::stub::FileTime);

impl FileTimeResult {
    pub(crate) fn new(file_time: fd_util::stub::FileTime) -> Self {
        Self(file_time)
    }

    pub(crate) fn as_native(&self) -> &fd_util::stub::FileTime {
        &self.0
    }
}

impl std::fmt::Debug for FileTimeResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("FileTimeResult").finish_non_exhaustive()
    }
}

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

#[derive(Debug)]
pub(crate) enum RealpathJobResult {
    // The completed job owns the native path until the guest requests it.
    Unpublished(Box<[u8]>),
    // The host c_buffer table owns the path and the job finalizer releases it.
    Published(HostHandle),
}

#[derive(Debug)]
pub(crate) enum JobPayload {
    Failed {
        errno: i32,
    },
    Sleep {
        duration_ms: i32,
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
    Bind {
        socket: Option<ResourceRef>,
        addr: Vec<u8>,
    },
    GetAddrInfo {
        host: OsString,
        result: Option<Vec<Box<[u8]>>>,
    },
    Realpath {
        path: OsString,
        result: Option<RealpathJobResult>,
    },
    #[cfg(unix)]
    SpawnUnix {
        path: OsString,
        args: Vec<OsString>,
        env: Vec<OsString>,
        options: SpawnOptions,
        stdio: [Option<ResourceRef>; 3],
        cwd: Option<OsString>,
        result: Option<OpenJobResource>,
    },
    #[cfg(windows)]
    SpawnWindows {
        command_line: OsString,
        env: Vec<u16>,
        options: SpawnOptions,
        stdio: [Option<ResourceRef>; 3],
        cwd: Option<OsString>,
        result: Option<OpenJobResource>,
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
