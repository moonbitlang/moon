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
use std::sync::{Arc, Mutex};

use crate::async_host::{AsyncHostResult, HostCBuffer};
use crate::async_sys::internal::fd_util;

#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
#[cfg(windows)]
use std::os::windows::io::{
    AsRawHandle, AsRawSocket, FromRawHandle, FromRawSocket, OwnedHandle, OwnedSocket, RawSocket,
};

pub(crate) type ResourceHandle = u64;
pub(crate) type HostHandle = ResourceHandle;
pub(crate) type FileResourceRef = Arc<FileResource>;

#[cfg(unix)]
type OwnedRawFile = OwnedFd;
#[cfg(windows)]
type OwnedRawFile = OwnedHandle;

#[derive(Debug)]
pub(crate) struct FileResource {
    raw: RawFileResource,
    // Native directory enumeration mutates cursor state on the opened resource.
    directory_cursor: Mutex<()>,
}

#[derive(Debug)]
enum RawFileResource {
    Invalid,
    File(OwnedRawFile),
    #[cfg(windows)]
    Socket(OwnedSocket),
}

impl FileResource {
    pub(crate) fn new(raw: fd_util::stub::RawFd) -> Self {
        if raw == invalid_raw_file() {
            return Self::invalid();
        }
        Self {
            raw: RawFileResource::File(owned_raw_file(raw)),
            directory_cursor: Mutex::new(()),
        }
    }

    #[cfg(windows)]
    pub(crate) fn new_socket(raw: RawSocket) -> Self {
        if raw == invalid_raw_socket() {
            return Self::invalid();
        }
        Self {
            raw: RawFileResource::Socket(owned_raw_socket(raw)),
            directory_cursor: Mutex::new(()),
        }
    }

    pub(crate) fn invalid() -> Self {
        Self {
            raw: RawFileResource::Invalid,
            directory_cursor: Mutex::new(()),
        }
    }

    pub(crate) fn is_invalid(&self) -> bool {
        matches!(self.raw, RawFileResource::Invalid)
    }

    pub(crate) fn raw_fd(&self) -> fd_util::stub::RawFd {
        match &self.raw {
            RawFileResource::Invalid => invalid_raw_file(),
            RawFileResource::File(raw) => raw_file(raw),
            #[cfg(windows)]
            RawFileResource::Socket(raw) => raw_socket(raw),
        }
    }

    pub(crate) fn lock_directory_cursor(&self) -> std::sync::MutexGuard<'_, ()> {
        self.directory_cursor.lock().unwrap()
    }
}

#[cfg(unix)]
fn invalid_raw_file() -> fd_util::stub::RawFd {
    -1
}

#[cfg(windows)]
fn invalid_raw_file() -> fd_util::stub::RawFd {
    windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE
}

#[cfg(windows)]
fn invalid_raw_socket() -> RawSocket {
    windows_sys::Win32::Networking::WinSock::INVALID_SOCKET as RawSocket
}

#[cfg(unix)]
fn owned_raw_file(raw: fd_util::stub::RawFd) -> OwnedRawFile {
    // FileResource takes ownership of handles returned by platform APIs.
    unsafe { OwnedFd::from_raw_fd(raw) }
}

#[cfg(windows)]
fn owned_raw_file(raw: fd_util::stub::RawFd) -> OwnedRawFile {
    // FileResource takes ownership of handles returned by platform APIs.
    unsafe { OwnedHandle::from_raw_handle(raw) }
}

#[cfg(windows)]
fn owned_raw_socket(raw: RawSocket) -> OwnedSocket {
    // FileResource takes ownership of sockets returned by platform APIs.
    unsafe { OwnedSocket::from_raw_socket(raw) }
}

#[cfg(unix)]
fn raw_file(raw: &OwnedRawFile) -> fd_util::stub::RawFd {
    raw.as_raw_fd()
}

#[cfg(windows)]
fn raw_file(raw: &OwnedRawFile) -> fd_util::stub::RawFd {
    raw.as_raw_handle()
}

#[cfg(windows)]
fn raw_socket(raw: &OwnedSocket) -> fd_util::stub::RawFd {
    raw.as_raw_socket() as fd_util::stub::RawFd
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenJobResult {
    pub(crate) fd: HostHandle,
    pub(crate) kind: i32,
    pub(crate) dev_id: u64,
    pub(crate) file_id: u64,
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub(crate) enum JobPayload {
    Sleep {
        duration_ms: i32,
    },
    Read {
        file: Option<FileResourceRef>,
        len: i32,
        position: i64,
        result: Option<Vec<u8>>,
    },
    Write {
        file: Option<FileResourceRef>,
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
    FileKindByPath {
        parent: Option<FileResourceRef>,
        path: OsString,
        follow_symlink: bool,
    },
    FileSize {
        file: Option<FileResourceRef>,
        result: i64,
    },
    FileTime {
        file: Option<FileResourceRef>,
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
        file: Option<FileResourceRef>,
        only_data: bool,
    },
    Flock {
        file: Option<FileResourceRef>,
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
        dir: Option<FileResourceRef>,
        buffer: HostCBuffer,
        len: i32,
        restart: bool,
    },
    Bind {
        socket: Option<FileResourceRef>,
        addr: Vec<u8>,
    },
    GetAddrInfo {
        host: OsString,
        result: Option<Vec<Box<[u8]>>>,
    },
}

pub(crate) trait FileResourceTable {
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
