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

//! Ports of `moonbitlang/async/src/internal/event_loop/{epoll,kqueue,iocp}.c`.
//! The native C surface calls this abstraction the event bus.

use crate::async_host::AsyncHostError;
use crate::async_sys::internal::fd_util::stub::RawFd;

#[cfg(unix)]
use std::os::fd::{AsRawFd, OwnedFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, OwnedHandle};
#[cfg(windows)]
use std::sync::Arc;

#[cfg(target_os = "linux")]
mod epoll;
#[cfg(windows)]
mod iocp;
#[cfg(target_os = "macos")]
mod kqueue;

#[cfg(target_os = "linux")]
pub(crate) use epoll::*;
#[cfg(windows)]
pub(crate) use iocp::*;
#[cfg(target_os = "macos")]
pub(crate) use kqueue::*;

pub(super) const EVENT_BUFFER_SIZE: usize = 1024;
#[cfg(unix)]
pub(crate) const READ_EVENT: i32 = 1;
#[cfg(unix)]
pub(crate) const WRITE_EVENT: i32 = 2;
#[cfg(target_os = "macos")]
pub(crate) const PROCESS_EVENT: i32 = 4;

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct KeventBuffer(Box<[libc::kevent]>);

#[cfg(target_os = "macos")]
// SAFETY: libc::kevent is plain kernel event storage. Its udata field prevents
// automatic Send, but moonrun always registers null udata and never dereferences
// values returned in that field. The buffer owns every event value.
unsafe impl Send for KeventBuffer {}

#[derive(Debug)]
pub(crate) struct PollInstance {
    #[cfg(unix)]
    fd: OwnedFd,
    #[cfg(windows)]
    fd: Arc<OwnedHandle>,
    // Unix pollers retain both buffers so waits do not allocate or initialize
    // 1024 native events before copying the ready subset on every call.
    #[cfg(target_os = "linux")]
    raw_events: Box<[libc::epoll_event]>,
    #[cfg(target_os = "macos")]
    raw_events: KeventBuffer,
    events: Vec<PollEvent>,
}

impl PollInstance {
    pub(crate) fn raw_fd(&self) -> RawFd {
        #[cfg(unix)]
        {
            self.fd.as_raw_fd()
        }
        #[cfg(windows)]
        {
            self.fd.as_raw_handle()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PollEvent {
    #[cfg(unix)]
    fd: RawFd,
    #[cfg(windows)]
    // IOCP completion keys are opaque values. Store the address bits rather
    // than a pointer so cached events carry no false pointer provenance.
    fd: usize,
    events: i32,
    #[cfg(windows)]
    io_result: usize,
    #[cfg(windows)]
    bytes_transferred: i32,
    #[cfg(windows)]
    worker_generation: Option<usize>,
}

pub(super) fn last_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_else(|| AsyncHostError::Inval.errno())
}

pub(super) fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(last_errno())
}

#[cfg(test)]
pub(crate) fn ported_symbols() -> Vec<crate::async_sys::PortedSymbol> {
    let mut symbols = Vec::new();
    #[cfg(target_os = "linux")]
    symbols.extend_from_slice(epoll::PORTED_SYMBOLS);
    #[cfg(target_os = "macos")]
    symbols.extend_from_slice(kqueue::PORTED_SYMBOLS);
    #[cfg(windows)]
    symbols.extend_from_slice(iocp::PORTED_SYMBOLS);
    symbols.retain(|symbol| {
        matches!(
            symbol.native_symbol,
            "moonbitlang_async_event_bus_create"
                | "moonbitlang_async_event_bus_destroy"
                | "moonbitlang_async_event_bus_register"
                | "moonbitlang_async_event_bus_register_pid"
                | "moonbitlang_async_event_bus_wait"
                | "moonbitlang_async_event_list_get"
                | "moonbitlang_async_event_get_fd"
                | "moonbitlang_async_event_get_events"
                | "moonbitlang_async_event_get_io_result"
                | "moonbitlang_async_event_get_bytes_transferred"
        )
    });
    symbols
}

#[cfg(test)]
#[cfg(unix)]
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
    fn event_bus_wait_reports_pipe_readiness() {
        let mut fds = [0; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let read_fd = fds[0];
        let write_fd = fds[1];

        let mut poll = poll_create().unwrap();
        poll_register(&poll, read_fd, true).unwrap();
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

        unsafe {
            libc::close(read_fd);
            libc::close(write_fd);
        }
    }

    #[test]
    fn event_bus_unregister_removes_pipe_readiness() {
        let mut fds = [0; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let read_fd = fds[0];
        let write_fd = fds[1];

        let mut poll = poll_create().unwrap();
        poll_register(&poll, read_fd, true).unwrap();
        poll_unregister(&poll, read_fd).unwrap();
        let byte = b"x";
        assert_eq!(
            unsafe { libc::write(write_fd, byte.as_ptr().cast(), byte.len()) },
            1
        );

        assert_eq!(poll_wait(&mut poll, 0).unwrap(), 0);

        unsafe {
            libc::close(read_fd);
            libc::close(write_fd);
        }
    }

    #[test]
    fn ported_symbols_reference_native_sources() {
        let async_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../third_party/moonbitlang_async");
        for symbol in PORTED_SYMBOLS {
            let source_path = async_root.join(symbol.source);
            let contents = std::fs::read_to_string(&source_path)
                .unwrap_or_else(|error| panic!("failed to read {:?}: {error}", source_path));
            assert!(
                contents.contains(symbol.native_symbol),
                "{:?} does not contain native symbol {}",
                source_path,
                symbol.native_symbol
            );
        }
    }
}
