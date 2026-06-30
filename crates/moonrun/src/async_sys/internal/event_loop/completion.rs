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

#[cfg(unix)]
use crate::async_host::{AsyncHostError, AsyncHostResult};
#[cfg(unix)]
use crate::async_sys::internal::event_loop::poll;
#[cfg(unix)]
use crate::async_sys::internal::fd_util::stub as fd_util;
#[cfg(unix)]
use crate::async_sys::internal::fd_util::stub::RawFd;

#[cfg(unix)]
#[derive(Debug)]
pub(crate) struct ThreadPoolCompletionNotifier {
    notify_recv: RawFd,
    notify_send: RawFd,
}

#[cfg(unix)]
impl ThreadPoolCompletionNotifier {
    pub(crate) fn new(poll: &poll::PollInstance) -> AsyncHostResult<(Self, RawFd)> {
        let fds = fd_util::pipe()?;

        if let Err(error) = fd_util::set_nonblocking(fds[0]) {
            close_fds(fds);
            return Err(error);
        }

        if let Err(error) = poll::poll_register(poll, fds[0], true) {
            close_fds(fds);
            return Err(error);
        }

        // The read end is transferred to AsyncHost's file table so poll
        // events report the same handle space as ordinary pipe/file fds.
        Ok((
            Self {
                notify_recv: fds[0],
                notify_send: fds[1],
            },
            fds[0],
        ))
    }

    pub(crate) fn notify(&self, job_id: i32) -> AsyncHostResult<()> {
        let bytes = job_id.to_ne_bytes();
        let mut offset = 0;
        loop {
            let n = unsafe {
                libc::write(
                    self.notify_send,
                    bytes[offset..].as_ptr().cast(),
                    bytes.len() - offset,
                )
            };
            if n > 0 {
                offset += n as usize;
                if offset == bytes.len() {
                    return Ok(());
                }
                continue;
            }
            if n == 0 {
                return Err(AsyncHostError::Inval);
            }
            let errno = last_errno();
            if errno == libc::EINTR {
                continue;
            }
            return Err(AsyncHostError::Native(errno));
        }
    }

    pub(crate) fn fetch(&self, dst: &mut [u8]) -> AsyncHostResult<usize> {
        if dst.is_empty() {
            return Ok(0);
        }
        loop {
            let n = unsafe { libc::read(self.notify_recv, dst.as_mut_ptr().cast(), dst.len()) };
            if n > 0 {
                return usize::try_from(n).map_err(|_| AsyncHostError::Fault);
            }
            if n == 0 {
                return Ok(0);
            }
            let errno = last_errno();
            if errno == libc::EINTR {
                continue;
            }
            if would_block(errno) {
                return Ok(0);
            }
            return Err(AsyncHostError::Native(errno));
        }
    }
}

#[cfg(unix)]
impl Drop for ThreadPoolCompletionNotifier {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.notify_send);
        }
    }
}

#[cfg(unix)]
fn close_fds(fds: [RawFd; 2]) {
    unsafe {
        libc::close(fds[0]);
        libc::close(fds[1]);
    }
}

#[cfg(unix)]
fn last_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_else(|| AsyncHostError::Inval.errno())
}

#[cfg(unix)]
fn would_block(errno: i32) -> bool {
    errno == libc::EAGAIN || errno == libc::EWOULDBLOCK
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn completion_pipe_is_close_on_exec() {
        let poll = poll::poll_create().unwrap();
        let (notifier, notify_recv) = ThreadPoolCompletionNotifier::new(&poll).unwrap();

        for fd in [notify_recv, notifier.notify_send] {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            assert!(flags >= 0);
            assert_ne!(flags & libc::FD_CLOEXEC, 0);
        }

        unsafe {
            libc::close(notify_recv);
        }
    }
}
