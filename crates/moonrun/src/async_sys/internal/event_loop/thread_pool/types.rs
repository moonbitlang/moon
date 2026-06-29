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

use crate::async_host::{AsyncHostError, AsyncHostResult, HostCBuffer};
use crate::async_sys::internal::fd_util;

pub(crate) type HostHandle = u64;

#[derive(Debug)]
pub(crate) struct HostFile {
    raw: fd_util::stub::RawFd,
    close_kind: HostFileCloseKind,
}

#[cfg(windows)]
unsafe impl Send for HostFile {}

#[derive(Debug, Clone, Copy)]
enum HostFileCloseKind {
    File,
    #[cfg(windows)]
    Socket,
}

impl HostFile {
    pub(crate) fn new(raw: fd_util::stub::RawFd) -> Self {
        Self {
            raw,
            close_kind: HostFileCloseKind::File,
        }
    }

    #[cfg(windows)]
    pub(crate) fn new_socket(raw: fd_util::stub::RawFd) -> Self {
        Self {
            raw,
            close_kind: HostFileCloseKind::Socket,
        }
    }

    pub(crate) fn invalid() -> Self {
        Self::new(invalid_raw_file())
    }

    pub(crate) fn is_invalid(&self) -> bool {
        is_invalid_raw_file(self.raw)
    }

    pub(crate) fn raw_fd(&self) -> fd_util::stub::RawFd {
        self.raw
    }

    pub(crate) fn duplicate(&self) -> AsyncHostResult<Self> {
        if self.is_invalid() {
            return Err(AsyncHostError::Badf);
        }
        duplicate_raw_file(self.raw).map(|raw| Self {
            raw,
            close_kind: self.close_kind,
        })
    }
}

impl Drop for HostFile {
    fn drop(&mut self) {
        if !self.is_invalid() {
            close_raw_file(self.raw, self.close_kind);
            self.raw = invalid_raw_file();
        }
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

fn is_invalid_raw_file(raw: fd_util::stub::RawFd) -> bool {
    raw == invalid_raw_file()
}

#[cfg(unix)]
fn close_raw_file(raw: fd_util::stub::RawFd, _close_kind: HostFileCloseKind) {
    unsafe {
        libc::close(raw);
    }
}

#[cfg(windows)]
fn close_raw_file(raw: fd_util::stub::RawFd, close_kind: HostFileCloseKind) {
    match close_kind {
        HostFileCloseKind::File => unsafe {
            windows_sys::Win32::Foundation::CloseHandle(raw);
        },
        HostFileCloseKind::Socket => unsafe {
            windows_sys::Win32::Networking::WinSock::closesocket(raw as usize);
        },
    }
}

#[cfg(unix)]
fn duplicate_raw_file(raw: fd_util::stub::RawFd) -> AsyncHostResult<fd_util::stub::RawFd> {
    let fd = unsafe { libc::fcntl(raw, libc::F_DUPFD_CLOEXEC, 0) };
    if fd < 0 {
        Err(last_native_error())
    } else {
        Ok(fd)
    }
}

#[cfg(windows)]
fn duplicate_raw_file(raw: fd_util::stub::RawFd) -> AsyncHostResult<fd_util::stub::RawFd> {
    use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let process = unsafe { GetCurrentProcess() };
    let mut duplicate: HANDLE = std::ptr::null_mut();
    if unsafe {
        DuplicateHandle(
            process,
            raw,
            process,
            &mut duplicate,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
    {
        Err(last_native_error())
    } else {
        Ok(duplicate)
    }
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
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
        fd: HostHandle,
        len: i32,
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
        buffer: HostCBuffer,
        len: i32,
        restart: bool,
    },
    Bind {
        socket: HostHandle,
        addr: Vec<u8>,
    },
    GetAddrInfo {
        host: OsString,
        result: Option<Vec<Box<[u8]>>>,
    },
}

pub(crate) trait HostFileTable {
    fn insert_file(&mut self, file: fd_util::stub::RawFd) -> AsyncHostResult<HostHandle>;

    fn is_invalid_file_handle(&self, handle: HostHandle) -> bool;

    #[cfg(windows)]
    fn with_borrowed_raw_file<T>(
        &mut self,
        handle: HostHandle,
        f: impl FnOnce(fd_util::stub::RawFd) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T>;

    fn with_raw_file<T>(
        &mut self,
        handle: HostHandle,
        f: impl FnOnce(fd_util::stub::RawFd) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T>;

    fn with_host_file_mut<T>(
        &mut self,
        handle: HostHandle,
        f: impl FnOnce(&mut HostFile) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T>;
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
