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
use crate::async_sys::internal::fd_util::stub as fd_util;
use crate::async_sys::internal::fd_util::stub::RawFd;
use std::collections::VecDeque;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, AtomicI64, Ordering},
};

// Outside the i32 completion-ID range, including IDs supplied by direct guests.
const NO_RETRY: i64 = i64::MIN;

#[derive(Debug, Default)]
struct PendingCompletions {
    jobs: VecDeque<i32>,
    // Register only Workers that enable retries. Each Worker owns its slot;
    // fetch temporarily upgrades the Weak to keep it alive while reading.
    // Expired registrations are removed during fetch or the next registration.
    retries: Vec<Weak<AtomicI64>>,
}

#[derive(Debug)]
pub(crate) struct CancellationRetryNotifier {
    pending: Arc<AtomicI64>,
    source: Arc<ThreadPoolCompletionNotifier>,
}

impl CancellationRetryNotifier {
    pub(crate) fn notify_from_signal(&self, completion_id: i32) {
        // Publish before waking. Repeated retries for the same current Job
        // occupy one slot, even when the guest stops consuming notifications.
        // The handler uses the existing Arc without changing reference counts.
        self.pending
            .store(i64::from(completion_id), Ordering::SeqCst);
        self.source.retry_scan_needed.store(true, Ordering::SeqCst);
        let _ = self.source.wake();
    }

    pub(crate) fn reset(&self) {
        // The guest has accepted completion and is reusing the Worker.
        self.pending.store(NO_RETRY, Ordering::SeqCst);
    }
}

#[derive(Debug)]
pub(crate) struct ThreadPoolCompletionNotifier {
    // Retain both ends until the last publisher exits. The guest receives a
    // duplicate reader, so closing its handle cannot invalidate these fds.
    notify_recv: OwnedFd,
    notify_send: OwnedFd,
    pending: Mutex<PendingCompletions>,
    wake_pending: AtomicBool,
    // A published retry needs inspection; its slot may since have been reset.
    retry_scan_needed: AtomicBool,
    // Run signals must not backpressure the process-wide signal broker. They
    // coalesce separately from worker IDs, with one wake byte while nonempty.
    // Both ends stay alive with the notifier, including during an in-flight send.
    signal_recv: OwnedFd,
    signal_send: OwnedFd,
    pending_signals: Mutex<u32>,
}

impl ThreadPoolCompletionNotifier {
    #[cfg(test)]
    pub(crate) fn fill_with_completions(&self, completion_id: i32) {
        self.notify(completion_id).unwrap();
        // Fill the actual wake pipe too: publishing must handle EAGAIN without
        // losing an ID or blocking. Do not depend on a platform's pipe capacity.
        loop {
            let written =
                unsafe { libc::write(self.notify_send.as_raw_fd(), b"x".as_ptr().cast(), 1) };
            if written == 1 {
                continue;
            }
            assert_eq!(written, -1);
            if last_errno() == libc::EINTR {
                continue;
            }
            assert!(would_block(last_errno()));
            break;
        }
        // Force the next publication to attempt a write to the full pipe.
        self.wake_pending.store(false, Ordering::SeqCst);
    }

    pub(crate) fn new() -> AsyncHostResult<(Self, RawFd)> {
        let signals = fd_util::pipe(true, true)?;
        let signal_recv = unsafe { OwnedFd::from_raw_fd(signals[0]) };
        let signal_send = unsafe { OwnedFd::from_raw_fd(signals[1]) };
        let fds = fd_util::pipe(true, true)?;
        let notify_recv = unsafe { OwnedFd::from_raw_fd(fds[0]) };
        let notify_send = unsafe { OwnedFd::from_raw_fd(fds[1]) };
        let guest_recv = notify_recv
            .try_clone()
            .map_err(|error| AsyncHostError::Native(error.raw_os_error().unwrap_or(libc::EIO)))?;

        Ok((
            Self {
                notify_recv,
                notify_send,
                pending: Mutex::new(PendingCompletions::default()),
                wake_pending: AtomicBool::new(false),
                retry_scan_needed: AtomicBool::new(false),
                signal_recv,
                signal_send,
                pending_signals: Mutex::new(0),
            },
            guest_recv.into_raw_fd(),
        ))
    }

    pub(crate) fn notify(&self, completion_id: i32) -> AsyncHostResult<()> {
        self.pending.lock().unwrap().jobs.push_back(completion_id);
        self.wake()
    }

    pub(crate) fn cancellation_retry(self: &Arc<Self>) -> CancellationRetryNotifier {
        let slot = Arc::new(AtomicI64::new(NO_RETRY));
        let mut pending = self.pending.lock().unwrap();
        pending.retries.retain(|slot| slot.strong_count() != 0);
        pending.retries.push(Arc::downgrade(&slot));
        CancellationRetryNotifier {
            pending: slot,
            source: Arc::clone(self),
        }
    }

    // Also called by the cancellation signal handler: no locks, allocation,
    // or blocking writes. An existing wake covers all subsequent publications.
    fn wake(&self) -> AsyncHostResult<()> {
        if self.wake_pending.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        loop {
            let written =
                unsafe { libc::write(self.notify_send.as_raw_fd(), b"x".as_ptr().cast(), 1) };
            if written == 1 {
                return Ok(());
            }
            let errno = last_errno();
            if written < 0 && errno == libc::EINTR {
                continue;
            }
            if written < 0 && would_block(errno) {
                return Ok(());
            }
            self.wake_pending.store(false, Ordering::SeqCst);
            return Err(AsyncHostError::Native(if written == 0 {
                libc::EIO
            } else {
                errno
            }));
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

    /// Called by the Run's single guest consumer. Publishers may run concurrently.
    pub(crate) fn fetch(&self, dst: &mut [u8]) -> AsyncHostResult<usize> {
        if dst.len() < 4 {
            return Ok(0);
        }
        let signal_bytes = self.fetch_signals(dst)?;
        let dst = &mut dst[signal_bytes..];
        if dst.len() < 4 {
            return Ok(signal_bytes);
        }

        // Drain the old wake BEFORE clearing its flag, then inspect pending
        // IDs. A publisher either joins this fetch or sets a new wake; clearing
        // the flag after inspecting IDs could lose a concurrent publication.
        loop {
            let mut wake_bytes = [0; 128];
            let n = unsafe {
                libc::read(
                    self.notify_recv.as_raw_fd(),
                    wake_bytes.as_mut_ptr().cast(),
                    wake_bytes.len(),
                )
            };
            if n >= 0 {
                if n < wake_bytes.len() as isize {
                    break;
                }
                continue;
            }
            let errno = last_errno();
            if errno == libc::EINTR {
                continue;
            }
            if would_block(errno) {
                break;
            }
            return Err(AsyncHostError::Native(errno));
        }
        self.wake_pending.store(false, Ordering::SeqCst);

        let mut pending = self.pending.lock().unwrap();
        let count = pending.jobs.len().min(dst.len() / 4);
        for (output, id) in dst.chunks_exact_mut(4).zip(pending.jobs.drain(..count)) {
            output.copy_from_slice(&id.to_ne_bytes());
        }
        let mut bytes = count * 4;
        // Scan retry slots only when there is room to return an ID. If normal
        // completions filled the buffer, leave the flag set so the next fetch
        // can inspect retries and the source stays ready in the meantime.
        if dst.len() - bytes >= 4 && self.retry_scan_needed.swap(false, Ordering::SeqCst) {
            pending.retries.retain(|slot| {
                let Some(slot) = slot.upgrade() else {
                    return false;
                };
                if bytes + 4 > dst.len() {
                    if slot.load(Ordering::SeqCst) != NO_RETRY {
                        self.retry_scan_needed.store(true, Ordering::SeqCst);
                    }
                } else {
                    let id = slot.swap(NO_RETRY, Ordering::SeqCst);
                    if id != NO_RETRY {
                        dst[bytes..bytes + 4].copy_from_slice(&(id as i32).to_ne_bytes());
                        bytes += 4;
                    }
                }
                true
            });
        }
        let remaining = !pending.jobs.is_empty() || self.retry_scan_needed.load(Ordering::SeqCst);
        drop(pending);
        if remaining {
            // A short guest buffer must not consume the source's readiness.
            self.wake()?;
        }
        Ok(signal_bytes + bytes)
    }
}

fn last_errno() -> i32 {
    // Keep the signal-handler path confined to atomics and libc calls.
    #[cfg(target_os = "linux")]
    unsafe {
        *libc::__errno_location()
    }
    #[cfg(target_os = "macos")]
    unsafe {
        *libc::__error()
    }
}

fn would_block(errno: i32) -> bool {
    errno == libc::EAGAIN || errno == libc::EWOULDBLOCK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completions_share_one_wakeup_and_survive_a_closed_guest_reader() {
        let (notifier, recv) = ThreadPoolCompletionNotifier::new().unwrap();
        // Guest handle closure must not close the handler's reader or cause
        // subsequent notification writes to generate SIGPIPE.
        drop(unsafe { OwnedFd::from_raw_fd(recv) });
        for id in 0..65536 {
            notifier.notify(id).unwrap();
        }
        let mut readable = 0;
        assert_eq!(
            unsafe {
                libc::ioctl(
                    notifier.notify_recv.as_raw_fd(),
                    libc::FIONREAD,
                    &mut readable,
                )
            },
            0
        );
        assert_eq!(readable, 1, "a burst shares one wake byte");

        let mut bytes = [0; 128];
        for first in (0_i32..65536).step_by(32) {
            assert_eq!(notifier.fetch(&mut bytes).unwrap(), bytes.len());
            for (offset, id) in bytes.chunks_exact(4).enumerate() {
                assert_eq!(
                    i32::from_ne_bytes(id.try_into().unwrap()),
                    first + offset as i32
                );
            }
        }
        assert_eq!(notifier.fetch(&mut bytes).unwrap(), 0);
    }

    #[test]
    fn retries_do_not_lock_and_remain_pending_after_partial_fetch() {
        use std::sync::mpsc;
        use std::time::Duration;

        let (notifier, recv) = ThreadPoolCompletionNotifier::new().unwrap();
        let _recv = unsafe { OwnedFd::from_raw_fd(recv) };
        let notifier = Arc::new(notifier);
        let first = notifier.cancellation_retry();
        let second = notifier.cancellation_retry();
        let pending = notifier.pending.lock().unwrap();
        let (sent_tx, sent_rx) = mpsc::channel();
        let publisher = std::thread::spawn(move || {
            for _ in 0..65536 {
                first.notify_from_signal(i32::MIN);
                second.notify_from_signal(i32::MAX);
            }
            sent_tx.send(()).unwrap();
            (first, second)
        });
        let published_without_lock = sent_rx.recv_timeout(Duration::from_secs(2));
        drop(pending);
        let (first, second) = publisher.join().unwrap();
        assert!(
            published_without_lock.is_ok(),
            "signal publication waited for the completion lock"
        );
        assert_eq!(notifier.pending.lock().unwrap().jobs.len(), 0);
        assert_eq!(notifier.pending.lock().unwrap().retries.len(), 2);

        let mut bytes = [0; 4];
        for expected in [i32::MIN, i32::MAX] {
            let mut poll = libc::pollfd {
                fd: recv,
                events: libc::POLLIN,
                revents: 0,
            };
            assert_eq!(unsafe { libc::poll(&mut poll, 1, 0) }, 1);
            assert_eq!(notifier.fetch(&mut bytes).unwrap(), 4);
            assert_eq!(i32::from_ne_bytes(bytes), expected);
        }
        assert_eq!(notifier.fetch(&mut bytes).unwrap(), 0);

        first.notify_from_signal(17);
        first.reset();
        second.notify_from_signal(19);
        drop(second);
        assert_eq!(
            notifier.fetch(&mut bytes).unwrap(),
            0,
            "reused and freed Workers leave no stale retries"
        );
        assert_eq!(notifier.pending.lock().unwrap().retries.len(), 1);
    }

    #[test]
    fn concurrent_publication_and_fetch_do_not_lose_wakeups() {
        use std::sync::{Barrier, mpsc};
        use std::time::{Duration, Instant};

        let (notifier, recv) = ThreadPoolCompletionNotifier::new().unwrap();
        let _recv = unsafe { OwnedFd::from_raw_fd(recv) };
        let notifier = Arc::new(notifier);
        let start = Arc::new(Barrier::new(6));
        let producers: Vec<_> = (0..4)
            .map(|producer| {
                let notifier = Arc::clone(&notifier);
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    for id in 0..1024 {
                        notifier.notify(producer * 1024 + id).unwrap();
                        std::thread::yield_now();
                    }
                })
            })
            .collect();
        let retry = notifier.cancellation_retry();
        let (ack_tx, ack_rx) = mpsc::channel();
        let retry_start = Arc::clone(&start);
        let retry_producer = std::thread::spawn(move || {
            retry_start.wait();
            for id in 4096..4224 {
                retry.notify_from_signal(id);
                // A new Job ID is published only after the old one is fetched;
                // duplicates for the same Job may otherwise legitimately coalesce.
                assert_eq!(ack_rx.recv_timeout(Duration::from_secs(5)).unwrap(), id);
            }
        });
        start.wait();
        let mut seen = [false; 4224];
        let mut received = 0;
        let deadline = Instant::now() + Duration::from_secs(5);
        while received < seen.len() && Instant::now() < deadline {
            let mut poll = libc::pollfd {
                fd: recv,
                events: libc::POLLIN,
                revents: 0,
            };
            assert!(unsafe { libc::poll(&mut poll, 1, 100) } >= 0);
            if poll.revents == 0 {
                continue;
            }
            let mut bytes = [0; 28];
            let fetched = notifier.fetch(&mut bytes).unwrap();
            for bytes in bytes[..fetched].chunks_exact(4) {
                let id = i32::from_ne_bytes(bytes.try_into().unwrap());
                assert!(!seen[id as usize], "duplicate notification {id}");
                seen[id as usize] = true;
                received += 1;
                if id >= 4096 {
                    ack_tx.send(id).unwrap();
                }
            }
        }
        for producer in producers {
            producer.join().unwrap();
        }
        retry_producer.join().unwrap();
        assert!(
            seen.into_iter().all(|seen| seen),
            "a publication lost its wakeup"
        );
    }

    #[test]
    fn full_pipe_retains_completions_and_coalesces_retries() {
        use std::os::fd::{FromRawFd, OwnedFd};

        let (notifier, recv) = ThreadPoolCompletionNotifier::new().unwrap();
        let _recv = unsafe { OwnedFd::from_raw_fd(recv) };
        let notifier = Arc::new(notifier);
        let retry = notifier.cancellation_retry();
        notifier.fill_with_completions(17);
        notifier.notify(23).unwrap();
        retry.notify_from_signal(19);
        retry.notify_from_signal(19);

        let mut poll = libc::pollfd {
            fd: recv,
            events: libc::POLLIN,
            revents: 0,
        };
        for expected in [17, 23, 19] {
            assert_eq!(unsafe { libc::poll(&mut poll, 1, 0) }, 1);
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
            notifier.notify_recv.as_raw_fd(),
            notifier.notify_send.as_raw_fd(),
            notifier.signal_recv.as_raw_fd(),
            notifier.signal_send.as_raw_fd(),
        ] {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            assert!(flags >= 0);
            assert_ne!(flags & libc::FD_CLOEXEC, 0);
        }

        for fd in [
            notify_recv,
            notifier.notify_send.as_raw_fd(),
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
