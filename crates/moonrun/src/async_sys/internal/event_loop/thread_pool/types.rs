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

use super::process::HostProcess;
use crate::async_host::{AsyncHostResult, types::Platform};

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
    pub(super) fn new(payload: JobPayload) -> Self {
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
            JobPayload::Sleep { duration_ms } => super::sleep::run_sleep_job(*duration_ms),
            _ => self.err = super::runner::unsupported_job_errno(),
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

pub(crate) fn platform() -> Platform {
    #[cfg(windows)]
    {
        Platform::Windows
    }
    #[cfg(target_os = "macos")]
    {
        Platform::MacOS
    }
    #[cfg(target_os = "linux")]
    {
        Platform::Linux
    }
}
