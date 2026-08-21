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

//! Host-owned file and network resources.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::fd_util;

#[cfg(unix)]
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
#[cfg(all(test, windows))]
use std::os::windows::io::AsRawSocket;
#[cfg(windows)]
use std::os::windows::io::{
    AsHandle, AsRawHandle, AsSocket, BorrowedHandle, BorrowedSocket, FromRawHandle, FromRawSocket,
    OwnedHandle, OwnedSocket, RawHandle, RawSocket,
};

pub(crate) type ResourceRef = Arc<Resource>;

/// A Resource returned by a Job before or after it is exposed through a Handle.
#[derive(Debug)]
pub(crate) enum ResourcePublication {
    Unpublished(Resource),
    Published(u64),
}

#[cfg(unix)]
type OwnedRawFile = OwnedFd;
#[cfg(windows)]
type OwnedRawFile = OwnedHandle;
#[cfg(unix)]
type OwnedRawSocket = OwnedFd;
#[cfg(windows)]
type OwnedRawSocket = OwnedSocket;
#[cfg(unix)]
type RawSocket = RawFd;

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
    // The payload is the source of truth for the Resource Class. Keeping TCP
    // and UDP as distinct variants makes mismatched class/socket state
    // unrepresentable.
    payload: ResourcePayload,
    // Native directory enumeration serializes cursor access on the opened resource.
    directory_cursor: Mutex<()>,
}

#[derive(Debug)]
enum ResourcePayload {
    Invalid,
    File {
        raw: OwnedRawFile,
        policy_path: Option<PathBuf>,
    },
    // Reserved process stdio handle. The guest can use it, but does not own it.
    StdioFile(isize),
    TcpSocket {
        raw: OwnedRawSocket,
        family: i32,
    },
    UdpSocket {
        raw: OwnedRawSocket,
        family: i32,
    },
}

impl Resource {
    pub(crate) fn new(raw: fd_util::stub::RawFd) -> Self {
        if raw == invalid_raw_file() {
            return Self::invalid();
        }
        Self::from_payload(ResourcePayload::File {
            raw: owned_raw_file(raw),
            policy_path: None,
        })
    }

    pub(crate) fn new_with_policy_path(
        raw: fd_util::stub::RawFd,
        policy_path: Option<PathBuf>,
    ) -> Self {
        if raw == invalid_raw_file() {
            return Self::invalid();
        }
        Self::from_payload(ResourcePayload::File {
            raw: owned_raw_file(raw),
            policy_path,
        })
    }

    pub(crate) fn stdio_file(raw: fd_util::stub::RawFd) -> Self {
        if raw == invalid_raw_file() {
            return Self::invalid();
        }
        Self::from_payload(ResourcePayload::StdioFile(raw as isize))
    }

    pub(crate) fn tcp_socket(raw: RawSocket, family: i32) -> Self {
        if raw == invalid_raw_socket() {
            return Self::invalid();
        }
        Self::from_payload(ResourcePayload::TcpSocket {
            raw: owned_raw_socket(raw),
            family,
        })
    }

    pub(crate) fn udp_socket(raw: RawSocket, family: i32) -> Self {
        if raw == invalid_raw_socket() {
            return Self::invalid();
        }
        Self::from_payload(ResourcePayload::UdpSocket {
            raw: owned_raw_socket(raw),
            family,
        })
    }

    pub(crate) fn invalid() -> Self {
        Self::from_payload(ResourcePayload::Invalid)
    }

    fn from_payload(payload: ResourcePayload) -> Self {
        Self {
            payload,
            directory_cursor: Mutex::new(()),
        }
    }

    pub(crate) fn is_invalid(&self) -> bool {
        matches!(&self.payload, ResourcePayload::Invalid)
    }

    pub(crate) fn resource_class(&self) -> ResourceClass {
        match &self.payload {
            ResourcePayload::TcpSocket { .. } => ResourceClass::TcpSocket,
            ResourcePayload::UdpSocket { .. } => ResourceClass::UdpSocket,
            ResourcePayload::Invalid
            | ResourcePayload::File { .. }
            | ResourcePayload::StdioFile(_) => ResourceClass::File,
        }
    }

    #[cfg(all(test, windows))]
    pub(crate) fn raw_identity(&self) -> isize {
        match &self.payload {
            ResourcePayload::Invalid => -1,
            ResourcePayload::File { raw, .. } => raw.as_handle().as_raw_handle() as isize,
            ResourcePayload::StdioFile(raw) => *raw,
            ResourcePayload::TcpSocket { raw, .. } => raw.as_socket().as_raw_socket() as isize,
            ResourcePayload::UdpSocket { raw, .. } => raw.as_socket().as_raw_socket() as isize,
        }
    }

    #[cfg(unix)]
    pub(crate) fn as_fd(&self) -> AsyncHostResult<BorrowedFd<'_>> {
        match &self.payload {
            ResourcePayload::Invalid => Err(AsyncHostError::Badf),
            ResourcePayload::File { raw, .. } => Ok(raw.as_fd()),
            ResourcePayload::StdioFile(_) => Err(AsyncHostError::Inval),
            ResourcePayload::TcpSocket { raw, .. } => Ok(raw.as_fd()),
            ResourcePayload::UdpSocket { raw, .. } => Ok(raw.as_fd()),
        }
    }

    #[cfg(unix)]
    pub(crate) fn as_file(&self) -> AsyncHostResult<FileRef<'_>> {
        match &self.payload {
            ResourcePayload::Invalid => Err(AsyncHostError::Badf),
            ResourcePayload::File { raw, .. } => Ok(FileRef::Borrowed(raw.as_fd())),
            ResourcePayload::StdioFile(raw) => i32::try_from(*raw)
                .map(FileRef::Stdio)
                .map_err(|_| AsyncHostError::Badf),
            ResourcePayload::TcpSocket { raw, .. } => Ok(FileRef::Borrowed(raw.as_fd())),
            ResourcePayload::UdpSocket { raw, .. } => Ok(FileRef::Borrowed(raw.as_fd())),
        }
    }

    #[cfg(windows)]
    pub(crate) fn as_handle(&self) -> AsyncHostResult<BorrowedHandle<'_>> {
        match &self.payload {
            ResourcePayload::Invalid => Err(AsyncHostError::Badf),
            ResourcePayload::File { raw, .. } => Ok(raw.as_handle()),
            ResourcePayload::StdioFile(_) => Err(AsyncHostError::Inval),
            ResourcePayload::TcpSocket { .. } | ResourcePayload::UdpSocket { .. } => {
                Err(AsyncHostError::Inval)
            }
        }
    }

    #[cfg(windows)]
    pub(crate) fn as_file(&self) -> AsyncHostResult<FileRef<'_>> {
        match &self.payload {
            ResourcePayload::Invalid => Err(AsyncHostError::Badf),
            ResourcePayload::File { raw, .. } => Ok(FileRef::Borrowed(raw.as_handle())),
            ResourcePayload::StdioFile(raw) => Ok(FileRef::Stdio(*raw as RawHandle)),
            ResourcePayload::TcpSocket { .. } | ResourcePayload::UdpSocket { .. } => {
                Err(AsyncHostError::Inval)
            }
        }
    }

    #[cfg(windows)]
    pub(crate) fn as_socket(&self) -> AsyncHostResult<BorrowedSocket<'_>> {
        match &self.payload {
            ResourcePayload::Invalid => Err(AsyncHostError::Badf),
            ResourcePayload::TcpSocket { raw, .. } => Ok(raw.as_socket()),
            ResourcePayload::UdpSocket { raw, .. } => Ok(raw.as_socket()),
            ResourcePayload::File { .. } | ResourcePayload::StdioFile(_) => {
                Err(AsyncHostError::Inval)
            }
        }
    }

    pub(crate) fn policy_path(&self) -> Option<&Path> {
        match &self.payload {
            ResourcePayload::File { policy_path, .. } => policy_path.as_deref(),
            ResourcePayload::Invalid
            | ResourcePayload::StdioFile(_)
            | ResourcePayload::TcpSocket { .. }
            | ResourcePayload::UdpSocket { .. } => None,
        }
    }

    pub(crate) fn socket_family(&self) -> Option<i32> {
        match &self.payload {
            ResourcePayload::TcpSocket { family, .. } => Some(*family),
            ResourcePayload::UdpSocket { family, .. } => Some(*family),
            ResourcePayload::Invalid
            | ResourcePayload::File { .. }
            | ResourcePayload::StdioFile(_) => None,
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

#[cfg(unix)]
fn invalid_raw_socket() -> RawSocket {
    -1
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

#[cfg(unix)]
fn owned_raw_socket(raw: RawSocket) -> OwnedRawSocket {
    // Resource takes ownership of sockets returned by platform APIs.
    unsafe { OwnedFd::from_raw_fd(raw) }
}

#[cfg(windows)]
fn owned_raw_socket(raw: RawSocket) -> OwnedRawSocket {
    // Resource takes ownership of sockets returned by platform APIs.
    unsafe { OwnedSocket::from_raw_socket(raw) }
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
