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

#[cfg(unix)]
mod sys {
    use super::{AsyncHostError, AsyncHostResult};

    pub(crate) struct HostEvent {
        read_fd: i32,
        write_fd: i32,
    }

    impl HostEvent {
        pub(crate) fn new() -> AsyncHostResult<Self> {
            let mut fds = [0; 2];
            if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
                return Err(last_error());
            }

            if let Err(error) = set_nonblocking(fds[0]).and_then(|()| set_nonblocking(fds[1])) {
                unsafe {
                    libc::close(fds[0]);
                    libc::close(fds[1]);
                }
                return Err(error);
            }

            Ok(Self {
                read_fd: fds[0],
                write_fd: fds[1],
            })
        }

        pub(crate) fn notify(&self) -> AsyncHostResult<()> {
            let byte = [1_u8];
            loop {
                let n = unsafe { libc::write(self.write_fd, byte.as_ptr().cast(), byte.len()) };
                if n == 1 {
                    return Ok(());
                }
                let error = errno();
                if error == libc::EINTR {
                    continue;
                }
                if would_block(error) {
                    return Ok(());
                }
                return Err(AsyncHostError::Native(error));
            }
        }

        pub(crate) fn clear(&self) -> AsyncHostResult<()> {
            let mut buf = [0_u8; 64];
            loop {
                let n = unsafe { libc::read(self.read_fd, buf.as_mut_ptr().cast(), buf.len()) };
                if n > 0 {
                    continue;
                }
                if n == 0 {
                    return Ok(());
                }
                let error = errno();
                if error == libc::EINTR {
                    continue;
                }
                if would_block(error) {
                    return Ok(());
                }
                return Err(AsyncHostError::Native(error));
            }
        }

        pub(crate) fn wait(&self, timeout_ms: i32) -> AsyncHostResult<()> {
            let mut pollfd = libc::pollfd {
                fd: self.read_fd,
                events: libc::POLLIN,
                revents: 0,
            };
            loop {
                let n = unsafe { libc::poll(&mut pollfd, 1, timeout_ms) };
                if n >= 0 {
                    return Ok(());
                }
                let error = errno();
                if error == libc::EINTR {
                    continue;
                }
                return Err(AsyncHostError::Native(error));
            }
        }
    }

    impl Drop for HostEvent {
        fn drop(&mut self) {
            unsafe {
                libc::close(self.read_fd);
                libc::close(self.write_fd);
            }
        }
    }

    fn set_nonblocking(fd: i32) -> AsyncHostResult<()> {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 {
            return Err(last_error());
        }
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(last_error());
        }
        Ok(())
    }

    fn last_error() -> AsyncHostError {
        AsyncHostError::Native(errno())
    }

    fn errno() -> i32 {
        #[cfg(target_os = "linux")]
        {
            unsafe { *libc::__errno_location() }
        }
        #[cfg(target_os = "macos")]
        {
            unsafe { *libc::__error() }
        }
    }

    fn would_block(error: i32) -> bool {
        error == libc::EAGAIN || error == libc::EWOULDBLOCK
    }
}

#[cfg(windows)]
mod sys {
    use std::ptr::null;

    use super::{AsyncHostError, AsyncHostResult};
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Threading::{
        CreateEventW, INFINITE, ResetEvent, SetEvent, WaitForSingleObject,
    };

    pub(crate) struct HostEvent {
        handle: HANDLE,
    }

    // Win32 event handles are process-wide kernel objects and are safe to wait
    // and signal from multiple host worker threads.
    unsafe impl Send for HostEvent {}
    unsafe impl Sync for HostEvent {}

    impl HostEvent {
        pub(crate) fn new() -> AsyncHostResult<Self> {
            let handle = unsafe { CreateEventW(null(), 1, 0, null()) };
            if handle.is_null() {
                return Err(last_error());
            }
            Ok(Self { handle })
        }

        pub(crate) fn notify(&self) -> AsyncHostResult<()> {
            if unsafe { SetEvent(self.handle) } == 0 {
                return Err(last_error());
            }
            Ok(())
        }

        pub(crate) fn clear(&self) -> AsyncHostResult<()> {
            if unsafe { ResetEvent(self.handle) } == 0 {
                return Err(last_error());
            }
            Ok(())
        }

        pub(crate) fn wait(&self, timeout_ms: i32) -> AsyncHostResult<()> {
            let timeout = if timeout_ms < 0 {
                INFINITE
            } else {
                timeout_ms as u32
            };
            match unsafe { WaitForSingleObject(self.handle, timeout) } {
                WAIT_OBJECT_0 | WAIT_TIMEOUT => Ok(()),
                WAIT_FAILED => Err(last_error()),
                _ => Err(last_error()),
            }
        }
    }

    impl Drop for HostEvent {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }

    fn last_error() -> AsyncHostError {
        AsyncHostError::Native(unsafe { GetLastError() } as i32)
    }
}

pub(crate) use sys::HostEvent;
