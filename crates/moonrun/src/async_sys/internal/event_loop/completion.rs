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
use crate::async_sys::internal::fd_util::stub as fd_util;
#[cfg(unix)]
use crate::async_sys::internal::fd_util::stub::RawFd;
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
#[cfg(unix)]
use std::sync::Mutex;

#[cfg(unix)]
#[derive(Debug)]
pub(crate) struct ThreadPoolCompletionNotifier {
    notify_recv: RawFd,
    notify_send: RawFd,
    // Run signals must not backpressure the process-wide signal broker. They
    // coalesce separately from worker IDs, with one wake byte while nonempty.
    // Both ends stay alive with the notifier, including during an in-flight send.
    signal_recv: OwnedFd,
    signal_send: OwnedFd,
    pending_signals: Mutex<u32>,
}

#[cfg(unix)]
impl ThreadPoolCompletionNotifier {
    #[cfg(test)]
    pub(crate) fn fill_with_completions(&self, completion_id: i32) {
        // Establish backpressure without depending on the platform's pipe capacity.
        let flags = unsafe { libc::fcntl(self.notify_send, libc::F_GETFL) };
        assert!(flags >= 0);
        assert_eq!(
            unsafe { libc::fcntl(self.notify_send, libc::F_SETFL, flags | libc::O_NONBLOCK) },
            0
        );
        let bytes = completion_id.to_ne_bytes();
        loop {
            let written =
                unsafe { libc::write(self.notify_send, bytes.as_ptr().cast(), bytes.len()) };
            if written == bytes.len() as isize {
                continue;
            }
            assert_eq!(written, -1);
            if last_errno() == libc::EINTR {
                continue;
            }
            assert!(would_block(last_errno()));
            break;
        }
        assert_eq!(
            unsafe { libc::fcntl(self.notify_send, libc::F_SETFL, flags) },
            0
        );
    }

    pub(crate) fn new() -> AsyncHostResult<(Self, RawFd)> {
        let signals = fd_util::pipe(true, true)?;
        let signal_recv = unsafe { OwnedFd::from_raw_fd(signals[0]) };
        let signal_send = unsafe { OwnedFd::from_raw_fd(signals[1]) };
        let fds = fd_util::pipe(true, false)?;

        // The read end is transferred to AsyncHost's file table so poll
        // events report the same handle space as ordinary pipe/file fds.
        Ok((
            Self {
                notify_recv: fds[0],
                notify_send: fds[1],
                signal_recv,
                signal_send,
                pending_signals: Mutex::new(0),
            },
            fds[0],
        ))
    }

    pub(crate) fn notify(&self, completion_id: i32) -> AsyncHostResult<()> {
        let bytes = completion_id.to_ne_bytes();
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

    pub(crate) fn signal_fd(&self) -> RawFd {
        self.signal_recv.as_raw_fd()
    }

    pub(crate) fn notify_signal(&self, signal_bit: u32) -> AsyncHostResult<()> {
        let mut pending = self.pending_signals.lock().unwrap();
        if *pending == 0 {
            // Serialize the empty-to-nonempty transition with fetch/discard.
            // This pipe contains only the wake byte, never individual events.
            loop {
                let written =
                    unsafe { libc::write(self.signal_send.as_raw_fd(), b"x".as_ptr().cast(), 1) };
                if written == 1 {
                    break;
                }
                if written == 0 {
                    return Err(AsyncHostError::Inval);
                }
                let errno = last_errno();
                if errno == libc::EINTR {
                    continue;
                }
                if would_block(errno) {
                    // An existing wake byte already makes the poller readable.
                    break;
                }
                return Err(AsyncHostError::Native(errno));
            }
        }
        *pending |= signal_bit;
        Ok(())
    }

    fn fetch_signals(&self, dst: &mut [u8]) -> AsyncHostResult<usize> {
        let mut pending = self.pending_signals.lock().unwrap();
        let mut bytes = 0;
        for output in dst.chunks_exact_mut(4) {
            if *pending == 0 {
                break;
            }
            let signal = pending.trailing_zeros();
            output.copy_from_slice(&((signal as i32) | i32::MIN).to_ne_bytes());
            *pending &= !(1 << signal);
            bytes += 4;
        }
        if bytes != 0 && *pending == 0 {
            // Leave the byte untouched after a partial fetch: the poller must
            // remain readable until every pending signal has been consumed.
            loop {
                let mut wake = 0_u8;
                let read =
                    unsafe { libc::read(self.signal_recv.as_raw_fd(), (&raw mut wake).cast(), 1) };
                if read == 1 {
                    break;
                }
                if read == 0 {
                    return Err(AsyncHostError::Inval);
                }
                let errno = last_errno();
                if errno == libc::EINTR {
                    continue;
                }
                return Err(AsyncHostError::Native(errno));
            }
        }
        Ok(bytes)
    }

    pub(crate) fn discard_signals(&self) {
        // Detachment may abandon accepted signals. Drain all bits and their
        // wake byte so restarting delivery cannot receive stale notifications.
        let _ = self.fetch_signals(&mut [0; u32::BITS as usize * 4]);
    }

    pub(crate) fn notify_from_signal(&self, completion_id: i32) {
        // Match thread_pool.c: worker cancellation handlers write job IDs into
        // the normal completion pipe, without locks or allocation. Native
        // relies on its scheduler's worker bound to keep this write from
        // blocking. FIXME: audit that bound for direct Wasm calls and Run
        // teardown; a full pipe blocks this handler if nothing drains it.
        unsafe {
            libc::write(
                self.notify_send,
                (&raw const completion_id).cast(),
                std::mem::size_of_val(&completion_id),
            );
        }
    }

    pub(crate) fn fetch(&self, dst: &mut [u8]) -> AsyncHostResult<usize> {
        if dst.is_empty() {
            return Ok(0);
        }
        let signal_bytes = self.fetch_signals(dst)?;
        let dst = &mut dst[signal_bytes..];
        if dst.is_empty() {
            return Ok(signal_bytes);
        }
        loop {
            let n = unsafe { libc::read(self.notify_recv, dst.as_mut_ptr().cast(), dst.len()) };
            if n > 0 {
                return usize::try_from(n)
                    .map(|bytes| signal_bytes + bytes)
                    .map_err(|_| AsyncHostError::Fault);
            }
            if n == 0 {
                return Ok(signal_bytes);
            }
            let errno = last_errno();
            if errno == libc::EINTR {
                continue;
            }
            if would_block(errno) {
                return Ok(signal_bytes);
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
    fn retry_notifications_preserve_pipe_order_and_duplicates() {
        use std::os::fd::{FromRawFd, OwnedFd};

        let (notifier, recv) = ThreadPoolCompletionNotifier::new().unwrap();
        let _recv = unsafe { OwnedFd::from_raw_fd(recv) };
        notifier.notify(17).unwrap();
        notifier.notify_from_signal(19);
        notifier.notify_from_signal(19);
        notifier.notify(23).unwrap();

        for expected in [17, 19, 19, 23] {
            let mut bytes = [0u8; 4];
            assert_eq!(notifier.fetch(&mut bytes).unwrap(), 4);
            assert_eq!(i32::from_ne_bytes(bytes), expected);
        }
        assert_eq!(notifier.fetch(&mut [0; 4]).unwrap(), 0);
    }

    #[test]
    fn completion_pipe_is_close_on_exec() {
        let (notifier, notify_recv) = ThreadPoolCompletionNotifier::new().unwrap();

        for fd in [
            notify_recv,
            notifier.notify_send,
            notifier.signal_recv.as_raw_fd(),
            notifier.signal_send.as_raw_fd(),
        ] {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            assert!(flags >= 0);
            assert_ne!(flags & libc::FD_CLOEXEC, 0);
        }

        for fd in [
            notifier.signal_recv.as_raw_fd(),
            notifier.signal_send.as_raw_fd(),
        ] {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            assert!(flags >= 0);
            assert_ne!(flags & libc::O_NONBLOCK, 0);
        }

        unsafe {
            libc::close(notify_recv);
        }
    }
}
