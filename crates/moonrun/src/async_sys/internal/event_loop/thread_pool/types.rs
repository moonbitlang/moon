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
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::async_host::{AsyncHostResult, HostCBuffer};
#[cfg(unix)]
use crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier;
use crate::async_sys::internal::fd_util;

#[cfg(unix)]
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{
    AsHandle, AsRawHandle, AsRawSocket, AsSocket, BorrowedHandle, BorrowedSocket, FromRawHandle,
    FromRawSocket, OwnedHandle, OwnedSocket, RawHandle, RawSocket,
};

pub(crate) type ResourceHandle = u64;
pub(crate) type HostHandle = ResourceHandle;
pub(crate) type ResourceRef = Arc<Resource>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SpawnOptions {
    #[cfg(unix)]
    pub(crate) child_signal_mask: libc::sigset_t,
    #[cfg(windows)]
    pub(crate) no_console_window: bool,
    #[cfg(windows)]
    pub(crate) is_orphan: bool,
}

#[cfg(unix)]
type OwnedRawFile = OwnedFd;
#[cfg(windows)]
type OwnedRawFile = OwnedHandle;

#[cfg(unix)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum FileRef<'a> {
    Borrowed(BorrowedFd<'a>),
    Stdio(RawFd),
}

#[cfg(unix)]
impl AsRawFd for FileRef<'_> {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            Self::Borrowed(fd) => fd.as_raw_fd(),
            Self::Stdio(fd) => *fd,
        }
    }
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum FileRef<'a> {
    Borrowed(BorrowedHandle<'a>),
    Stdio(RawHandle),
}

#[cfg(windows)]
impl AsRawHandle for FileRef<'_> {
    fn as_raw_handle(&self) -> RawHandle {
        match self {
            Self::Borrowed(handle) => handle.as_raw_handle(),
            Self::Stdio(handle) => *handle,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResourceClass {
    File,
    TcpSocket,
    UdpSocket,
}

impl ResourceClass {
    pub(crate) fn is_socket(self) -> bool {
        matches!(self, Self::TcpSocket | Self::UdpSocket)
    }
}

#[derive(Debug)]
pub(crate) struct Resource {
    raw: RawResource,
    class: ResourceClass,
    policy_path: Option<PathBuf>,
    socket_family: Option<i32>,
    // Native directory enumeration mutates cursor state on the opened resource.
    directory_cursor: Mutex<()>,
}

#[derive(Debug)]
enum RawResource {
    Invalid,
    File(OwnedRawFile),
    // Reserved process stdio handle. The guest can use it, but does not own it.
    StdioFile(isize),
    #[cfg(windows)]
    Socket(OwnedSocket),
}

impl Resource {
    pub(crate) fn new(raw: fd_util::stub::RawFd) -> Self {
        if raw == invalid_raw_file() {
            return Self::invalid();
        }
        Self {
            raw: RawResource::File(owned_raw_file(raw)),
            class: ResourceClass::File,
            policy_path: None,
            socket_family: None,
            directory_cursor: Mutex::new(()),
        }
    }

    pub(crate) fn new_with_policy_path(
        raw: fd_util::stub::RawFd,
        policy_path: Option<PathBuf>,
    ) -> Self {
        if raw == invalid_raw_file() {
            return Self::invalid();
        }
        Self {
            raw: RawResource::File(owned_raw_file(raw)),
            class: ResourceClass::File,
            policy_path,
            socket_family: None,
            directory_cursor: Mutex::new(()),
        }
    }

    pub(crate) fn stdio_file(raw: fd_util::stub::RawFd) -> Self {
        if raw == invalid_raw_file() {
            return Self::invalid();
        }
        Self {
            raw: RawResource::StdioFile(raw as isize),
            class: ResourceClass::File,
            policy_path: None,
            socket_family: None,
            directory_cursor: Mutex::new(()),
        }
    }

    #[cfg(unix)]
    pub(crate) fn new_socket(raw: fd_util::stub::RawFd, class: ResourceClass, family: i32) -> Self {
        debug_assert!(class.is_socket());
        if raw == invalid_raw_file() {
            return Self::invalid();
        }
        Self {
            raw: RawResource::File(owned_raw_file(raw)),
            class,
            policy_path: None,
            socket_family: Some(family),
            directory_cursor: Mutex::new(()),
        }
    }

    #[cfg(windows)]
    pub(crate) fn new_socket(raw: RawSocket, class: ResourceClass, family: i32) -> Self {
        debug_assert!(class.is_socket());
        if raw == invalid_raw_socket() {
            return Self::invalid();
        }
        Self {
            raw: RawResource::Socket(owned_raw_socket(raw)),
            class,
            policy_path: None,
            socket_family: Some(family),
            directory_cursor: Mutex::new(()),
        }
    }

    pub(crate) fn invalid() -> Self {
        Self {
            raw: RawResource::Invalid,
            class: ResourceClass::File,
            policy_path: None,
            socket_family: None,
            directory_cursor: Mutex::new(()),
        }
    }

    pub(crate) fn is_invalid(&self) -> bool {
        matches!(self.raw, RawResource::Invalid)
    }

    pub(crate) fn resource_class(&self) -> ResourceClass {
        self.class
    }

    pub(crate) fn raw_identity(&self) -> isize {
        match &self.raw {
            RawResource::Invalid => -1,
            #[cfg(unix)]
            RawResource::File(raw) => raw.as_fd().as_raw_fd() as isize,
            #[cfg(windows)]
            RawResource::File(raw) => raw.as_handle().as_raw_handle() as isize,
            RawResource::StdioFile(raw) => *raw,
            #[cfg(windows)]
            RawResource::Socket(raw) => raw.as_socket().as_raw_socket() as isize,
        }
    }

    #[cfg(unix)]
    pub(crate) fn as_fd(&self) -> AsyncHostResult<BorrowedFd<'_>> {
        match &self.raw {
            RawResource::Invalid => Err(crate::async_host::AsyncHostError::Badf),
            RawResource::File(raw) => Ok(raw.as_fd()),
            RawResource::StdioFile(_) => Err(crate::async_host::AsyncHostError::Inval),
        }
    }

    #[cfg(unix)]
    pub(crate) fn as_file(&self) -> AsyncHostResult<FileRef<'_>> {
        match &self.raw {
            RawResource::Invalid => Err(crate::async_host::AsyncHostError::Badf),
            RawResource::File(raw) => Ok(FileRef::Borrowed(raw.as_fd())),
            RawResource::StdioFile(raw) => i32::try_from(*raw)
                .map(FileRef::Stdio)
                .map_err(|_| crate::async_host::AsyncHostError::Badf),
        }
    }

    #[cfg(windows)]
    pub(crate) fn as_handle(&self) -> AsyncHostResult<BorrowedHandle<'_>> {
        match &self.raw {
            RawResource::Invalid => Err(crate::async_host::AsyncHostError::Badf),
            RawResource::File(raw) => Ok(raw.as_handle()),
            RawResource::StdioFile(_) => Err(crate::async_host::AsyncHostError::Inval),
            RawResource::Socket(_) => Err(crate::async_host::AsyncHostError::Inval),
        }
    }

    #[cfg(windows)]
    pub(crate) fn as_file(&self) -> AsyncHostResult<FileRef<'_>> {
        match &self.raw {
            RawResource::Invalid => Err(crate::async_host::AsyncHostError::Badf),
            RawResource::File(raw) => Ok(FileRef::Borrowed(raw.as_handle())),
            RawResource::StdioFile(raw) => Ok(FileRef::Stdio(*raw as RawHandle)),
            RawResource::Socket(_) => Err(crate::async_host::AsyncHostError::Inval),
        }
    }

    #[cfg(windows)]
    pub(crate) fn as_socket(&self) -> AsyncHostResult<BorrowedSocket<'_>> {
        match &self.raw {
            RawResource::Invalid => Err(crate::async_host::AsyncHostError::Badf),
            RawResource::Socket(raw) => Ok(raw.as_socket()),
            RawResource::File(_) | RawResource::StdioFile(_) => {
                Err(crate::async_host::AsyncHostError::Inval)
            }
        }
    }

    pub(crate) fn policy_path(&self) -> Option<&Path> {
        self.policy_path.as_deref()
    }

    pub(crate) fn socket_family(&self) -> Option<i32> {
        self.socket_family
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
    // Resource takes ownership of handles returned by platform APIs.
    unsafe { OwnedFd::from_raw_fd(raw) }
}

#[cfg(windows)]
fn owned_raw_file(raw: fd_util::stub::RawFd) -> OwnedRawFile {
    // Resource takes ownership of handles returned by platform APIs.
    unsafe { OwnedHandle::from_raw_handle(raw) }
}

#[cfg(windows)]
fn owned_raw_socket(raw: RawSocket) -> OwnedSocket {
    // Resource takes ownership of sockets returned by platform APIs.
    unsafe { OwnedSocket::from_raw_socket(raw) }
}

#[derive(Debug)]
pub(crate) struct OpenJobResult {
    pub(crate) resource: OpenJobResource,
    pub(crate) kind: i32,
    pub(crate) dev_id: u64,
    pub(crate) file_id: u64,
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
pub(crate) enum JobPayload {
    Failed {
        errno: i32,
    },
    Sleep {
        duration_ms: i32,
    },
    Read {
        file: Option<ResourceRef>,
        len: i32,
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
        result: Option<OpenJobResult>,
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
        buffer: HostCBuffer,
        len: i32,
        restart: bool,
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
        result: Option<Box<[u8]>>,
        result_handle: Option<HostHandle>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdio_resource_remains_explicitly_unowned() {
        #[cfg(unix)]
        let resource = Resource::stdio_file(0);
        #[cfg(windows)]
        let resource = Resource::stdio_file(1usize as RawHandle);

        assert!(matches!(resource.as_file(), Ok(FileRef::Stdio(_))));
        #[cfg(unix)]
        assert!(matches!(
            resource.as_fd(),
            Err(crate::async_host::AsyncHostError::Inval)
        ));
        #[cfg(windows)]
        assert!(matches!(
            resource.as_handle(),
            Err(crate::async_host::AsyncHostError::Inval)
        ));
    }
}
