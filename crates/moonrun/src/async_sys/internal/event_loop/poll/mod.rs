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

use crate::async_host::AsyncHostError;
use crate::async_sys::internal::fd_util::stub::RawFd;

#[cfg(target_os = "linux")]
mod epoll;
#[cfg(windows)]
mod iocp;
#[cfg(target_os = "macos")]
mod kqueue;

#[cfg(target_os = "linux")]
#[allow(unused_imports)]
pub(crate) use epoll::*;
#[cfg(windows)]
#[allow(unused_imports)]
pub(crate) use iocp::*;
#[cfg(target_os = "macos")]
#[allow(unused_imports)]
pub(crate) use kqueue::*;

pub(super) const EVENT_BUFFER_SIZE: usize = 1024;
#[allow(dead_code)]
pub(super) const READ_EVENT: i32 = 1;
#[allow(dead_code)]
pub(super) const WRITE_EVENT: i32 = 2;
#[allow(dead_code)]
pub(super) const PROCESS_EVENT: i32 = 4;

#[cfg(test)]
#[cfg(target_os = "linux")]
pub(crate) const PORTED_SYMBOLS: &[crate::async_sys::PortedSymbol] = epoll::PORTED_SYMBOLS;

#[cfg(test)]
#[cfg(target_os = "macos")]
pub(crate) const PORTED_SYMBOLS: &[crate::async_sys::PortedSymbol] = kqueue::PORTED_SYMBOLS;

#[cfg(test)]
#[cfg(windows)]
pub(crate) const PORTED_SYMBOLS: &[crate::async_sys::PortedSymbol] = iocp::PORTED_SYMBOLS;

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct PollInstance {
    fd: RawFd,
    events: Vec<PollEvent>,
}

impl PollInstance {
    #[allow(dead_code)]
    pub(crate) fn raw_fd(&self) -> RawFd {
        self.fd
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct PollEvent {
    fd: RawFd,
    events: i32,
    #[cfg(windows)]
    io_result: *mut windows_sys::Win32::System::IO::OVERLAPPED,
    #[cfg(windows)]
    bytes_transferred: i32,
}

impl Drop for PollInstance {
    fn drop(&mut self) {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        unsafe {
            libc::close(self.fd);
        }
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.fd);
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) fn last_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_else(|| AsyncHostError::Inval.errno())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(last_errno())
}

#[cfg(windows)]
pub(super) fn last_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_else(|| AsyncHostError::Inval.errno())
}

#[cfg(windows)]
pub(super) fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(last_errno())
}

// Android would take the native C `__linux__` epoll path, but it is outside the
// V8-backed moonrun async MVP. Keep the cfg split explicit instead of treating
// every Unix-like target as supported.

#[cfg(test)]
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn event_list_get_rejects_missing_event() {
        let poll = poll_create().unwrap();

        assert_eq!(
            event_list_get(&poll, 0).copied(),
            Err(AsyncHostError::Fault)
        );
    }

    #[test]
    fn poll_wait_reports_pipe_readiness() {
        let mut fds = [0; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let read_fd = fds[0];
        let write_fd = fds[1];

        let mut poll = poll_create().unwrap();
        poll_register(&poll, read_fd, 0, READ_EVENT, false).unwrap();
        let byte = b"x";
        assert_eq!(
            unsafe { libc::write(write_fd, byte.as_ptr().cast(), byte.len()) },
            1
        );

        let count = poll_wait(&mut poll, 100).unwrap();
        assert_eq!(count, 1);
        let event = *event_list_get(&poll, 0).unwrap();
        assert_eq!(event_get_fd(&event), read_fd);
        assert_eq!(event_get_events(&event) & READ_EVENT, READ_EVENT);

        poll_remove(&poll, read_fd, READ_EVENT).unwrap();
        unsafe {
            libc::close(read_fd);
            libc::close(write_fd);
        }
    }
}
