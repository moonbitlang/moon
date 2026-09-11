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
use crate::resource::ResourceRef;

/// A Worker's acquired async pipe writer, independent of the guest scheduler.
/// Each successful notification appends one little-endian i32 record.
#[derive(Debug)]
pub(crate) struct PipeCompletionNotifier {
    writer: ResourceRef,
    #[cfg(windows)]
    event: std::os::windows::io::OwnedHandle,
}

impl PipeCompletionNotifier {
    pub(crate) fn new(writer: ResourceRef) -> AsyncHostResult<Self> {
        // Provenance also rules out arbitrary file writes that would bypass
        // filesystem policy. A blocking writer could strand Worker teardown.
        if !writer.is_async_pipe_writer() {
            return Err(AsyncHostError::Inval);
        }
        #[cfg(windows)]
        let event = {
            use std::os::windows::io::{FromRawHandle, OwnedHandle};
            use windows_sys::Win32::System::Threading::CreateEventW;
            let raw = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
            if raw.is_null() {
                return Err(AsyncHostError::Native(unsafe {
                    windows_sys::Win32::Foundation::GetLastError() as i32
                }));
            }
            unsafe { OwnedHandle::from_raw_handle(raw) }
        };
        Ok(Self {
            writer,
            #[cfg(windows)]
            event,
        })
    }

    #[cfg(unix)]
    pub(crate) fn notify(
        &self,
        completion_id: i32,
        is_stopping: &dyn Fn() -> bool,
    ) -> AsyncHostResult<()> {
        use std::os::fd::AsRawFd;

        let fd = self.writer.as_file()?.as_raw_fd();
        let bytes = completion_id.to_le_bytes();
        loop {
            if is_stopping() {
                return Err(AsyncHostError::Inval);
            }
            let n = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
            if n == bytes.len() as isize {
                return Ok(());
            }
            // Four bytes fit in PIPE_BUF, so a nonblocking write is atomic.
            if n >= 0 {
                return Err(AsyncHostError::Io);
            }
            let errno = last_errno();
            if errno == libc::EINTR {
                continue;
            }
            if errno != libc::EAGAIN && errno != libc::EWOULDBLOCK {
                return Err(AsyncHostError::Native(errno));
            }
            let mut pollfd = libc::pollfd {
                fd,
                events: libc::POLLOUT,
                revents: 0,
            };
            // Bound the wait so free_worker can stop delivery even when the
            // reader is still open but no coroutine will ever drain it.
            if unsafe { libc::poll(&mut pollfd, 1, 50) } < 0 {
                let errno = last_errno();
                if errno != libc::EINTR {
                    return Err(AsyncHostError::Native(errno));
                }
            }
        }
    }

    #[cfg(windows)]
    pub(crate) fn notify(
        &self,
        completion_id: i32,
        is_stopping: &dyn Fn() -> bool,
    ) -> AsyncHostResult<()> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::{
            ERROR_IO_PENDING, GetLastError, WAIT_OBJECT_0, WAIT_TIMEOUT,
        };
        use windows_sys::Win32::Storage::FileSystem::WriteFile;
        use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
        use windows_sys::Win32::System::Threading::{ResetEvent, WaitForSingleObject};

        if is_stopping() {
            return Err(AsyncHostError::Inval);
        }
        let writer = self.writer.as_file()?.as_raw_handle();
        let event = self.event.as_raw_handle();
        if unsafe { ResetEvent(event) } == 0 {
            return Err(AsyncHostError::Native(unsafe { GetLastError() as i32 }));
        }
        let bytes = completion_id.to_le_bytes();
        let mut transferred = 0;
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        // Suppress IOCP packets for this private OVERLAPPED, including when
        // the guest has already registered the writer with its own poller.
        overlapped.hEvent = event.map_addr(|address| address | 1);
        let done = unsafe {
            WriteFile(
                writer,
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut transferred,
                &mut overlapped,
            )
        };
        if done == 0 {
            let errno = unsafe { GetLastError() };
            if errno != ERROR_IO_PENDING {
                return Err(AsyncHostError::Native(errno as i32));
            }
            loop {
                let wait = unsafe { WaitForSingleObject(event, 50) };
                if wait == WAIT_OBJECT_0 {
                    break;
                }
                if is_stopping() || wait != WAIT_TIMEOUT {
                    // Cancellation alone does not release the kernel's borrow
                    // of bytes/OVERLAPPED. Drain it before either leaves scope.
                    unsafe {
                        CancelIoEx(writer, &overlapped);
                        GetOverlappedResult(writer, &overlapped, &mut transferred, 1);
                    }
                    return Err(AsyncHostError::Io);
                }
            }
        }
        if unsafe { GetOverlappedResult(writer, &overlapped, &mut transferred, 1) } == 0 {
            return Err(AsyncHostError::Native(unsafe { GetLastError() as i32 }));
        }
        if transferred != bytes.len() as u32 {
            return Err(AsyncHostError::Io);
        }
        Ok(())
    }
}

#[cfg(unix)]
fn last_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EIO)
}
