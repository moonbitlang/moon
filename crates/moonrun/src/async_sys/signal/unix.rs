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

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::{Arc, Mutex, Weak};

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::event_loop::{
    ThreadPoolCompletionNotifier,
    thread_pool::{JobCancellation, JobCancellationOverride},
};
use crate::async_sys::internal::fd_util::stub as fd_util;
use crate::run_signal::{SignalReceiver, SigwaitTargetGuard, signal_mask};

#[derive(Clone, Debug)]
pub(crate) struct SigwaitTarget {
    signals: u32,
    state: Arc<Mutex<SigwaitState>>,
    wake: Arc<OwnedFd>,
}

impl SigwaitTarget {
    pub(crate) fn send(&self, bit: u32) -> AsyncHostResult<bool> {
        if self.signals & bit == 0 {
            return Ok(false);
        }

        let mut state = self.state.lock().map_err(|_| AsyncHostError::Inval)?;
        if state.cancelled {
            return Ok(false);
        }

        // Standard signals coalesce while pending. Holding the state lock
        // through the nonblocking write makes signal delivery and cancellation
        // ordered: an accepted signal is always observed before cancellation.
        if state.pending & bit != 0 {
            return Ok(true);
        }
        state.pending |= bit;
        match write_wakeup(&self.wake) {
            Ok(true) => Ok(true),
            Ok(false) => {
                state.pending &= !bit;
                Ok(false)
            }
            Err(error) => {
                state.pending &= !bit;
                Err(error)
            }
        }
    }
}

#[derive(Debug, Default)]
struct SigwaitState {
    pending: u32,
    cancelled: bool,
}

#[derive(Debug)]
struct SigwaitCancellation {
    state: Arc<Mutex<SigwaitState>>,
    wake: Weak<OwnedFd>,
}

impl JobCancellationOverride for SigwaitCancellation {
    fn cancel(&self) -> AsyncHostResult<i32> {
        let mut state = self.state.lock().map_err(|_| AsyncHostError::Inval)?;
        if state.cancelled {
            return Ok(0);
        }
        state.cancelled = true;
        if let Some(wake) = self.wake.upgrade()
            && let Err(error) = write_wakeup(&wake)
        {
            state.cancelled = false;
            return Err(error);
        }
        Ok(0)
    }
}

pub(crate) struct SigwaitJob {
    _target: SigwaitTargetGuard,
    state: Arc<Mutex<SigwaitState>>,
    cancellation: JobCancellation,
    wake: OwnedFd,
    notifier: Arc<ThreadPoolCompletionNotifier>,
}

impl std::fmt::Debug for SigwaitJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigwaitJob").finish_non_exhaustive()
    }
}

impl SigwaitJob {
    pub(crate) fn cancellation_override(&self) -> JobCancellation {
        Arc::clone(&self.cancellation)
    }

    pub(crate) fn run(&mut self) -> AsyncHostResult<i64> {
        run_sigwait_job(self)
    }
}

pub(crate) fn make_sigwait_job(
    receiver: &SignalReceiver,
    signals: &[i32],
    notifier: Arc<ThreadPoolCompletionNotifier>,
) -> AsyncHostResult<SigwaitJob> {
    let [wake_recv, wake_send] = fd_util::pipe(false, true)?;
    let wake_recv = unsafe { OwnedFd::from_raw_fd(wake_recv) };
    let wake_send = unsafe { OwnedFd::from_raw_fd(wake_send) };
    let state = Arc::new(Mutex::new(SigwaitState::default()));
    let wake_send = Arc::new(wake_send);
    let target = receiver
        .attach_target(SigwaitTarget {
            signals: signal_mask(signals),
            state: Arc::clone(&state),
            wake: Arc::clone(&wake_send),
        })
        .ok_or(AsyncHostError::Inval)?;
    let cancellation = Arc::new(SigwaitCancellation {
        state: Arc::clone(&state),
        wake: Arc::downgrade(&wake_send),
    });
    Ok(SigwaitJob {
        _target: target,
        state,
        cancellation,
        wake: wake_recv,
        notifier,
    })
}

// The process broker owns the real sigwait. This per-Run Job preserves the
// guest's Job lifecycle while receiving forwarded signals through its pipe.
// Its optional cancellation override wakes the same pipe, preserving native
// Job cancellation semantics without relying on an interrupt arriving while
// read(2) happens to be blocked.
fn run_sigwait_job(job: &mut SigwaitJob) -> AsyncHostResult<i64> {
    loop {
        let mut signals = {
            let mut state = job.state.lock().map_err(|_| AsyncHostError::Inval)?;
            if state.pending == 0 && state.cancelled {
                return Ok(0);
            }
            std::mem::take(&mut state.pending)
        };
        while signals != 0 {
            let signal = signals.trailing_zeros();
            signals &= signals - 1;
            let completion_id = (signal | (1_u32 << 31)) as i32;
            job.notifier.notify(completion_id)?;
        }

        let mut wake = [0_u8; 64];
        let received =
            unsafe { libc::read(job.wake.as_raw_fd(), wake.as_mut_ptr().cast(), wake.len()) };
        if received > 0 {
            continue;
        }
        if received == 0 {
            return Ok(0);
        }
        return Err(AsyncHostError::Native(last_native_errno()));
    }
}

fn write_wakeup(wake: &OwnedFd) -> AsyncHostResult<bool> {
    loop {
        let byte = 0_u8;
        let written = unsafe { libc::write(wake.as_raw_fd(), std::ptr::from_ref(&byte).cast(), 1) };
        if written == 1 {
            return Ok(true);
        }
        if written == 0 {
            return Err(AsyncHostError::Inval);
        }
        let errno = last_native_errno();
        if errno == libc::EINTR {
            continue;
        }
        if errno == libc::EAGAIN || errno == libc::EWOULDBLOCK {
            return Ok(true);
        }
        if errno == libc::EPIPE || errno == libc::EBADF {
            return Ok(false);
        }
        return Err(AsyncHostError::Native(errno));
    }
}

pub(super) fn set_global_cancellation_signals(
    receiver: &SignalReceiver,
    all_signals: &[i32],
    signals: &[i32],
) -> AsyncHostResult<()> {
    receiver.configure(all_signals, signals);
    Ok(())
}

pub(crate) fn init_thread_pool_signal_mask() -> AsyncHostResult<libc::sigset_t> {
    let mut signals_to_block = empty_signal_set()?;
    check_signal_call(unsafe { libc::sigaddset(&mut signals_to_block, libc::SIGCHLD) })?;
    let mut old = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    check_pthread_call(unsafe {
        libc::pthread_sigmask(libc::SIG_BLOCK, &signals_to_block, &mut old)
    })?;
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
    Ok(old)
}

pub(crate) fn restore_thread_pool_signal_mask(old: &libc::sigset_t) -> AsyncHostResult<()> {
    check_pthread_call(unsafe {
        libc::pthread_sigmask(libc::SIG_SETMASK, old, std::ptr::null_mut())
    })
}

pub(crate) fn set_worker_thread_signal_mask() -> AsyncHostResult<libc::sigset_t> {
    let mut worker_mask = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    check_signal_call(unsafe { libc::sigfillset(&mut worker_mask) })?;
    check_signal_call(unsafe { libc::sigdelset(&mut worker_mask, libc::SIGUSR2) })?;
    let mut old = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    check_pthread_call(unsafe {
        libc::pthread_sigmask(libc::SIG_SETMASK, &worker_mask, &mut old)
    })?;
    Ok(old)
}

pub(super) fn signal_int() -> i32 {
    libc::SIGINT
}

pub(super) fn signal_term() -> i32 {
    libc::SIGTERM
}

pub(super) fn signal_hup() -> i32 {
    libc::SIGHUP
}

pub(super) fn signal_break() -> i32 {
    -1
}

fn empty_signal_set() -> AsyncHostResult<libc::sigset_t> {
    let mut set = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    check_signal_call(unsafe { libc::sigemptyset(&mut set) })?;
    Ok(set)
}

fn check_signal_call(result: i32) -> AsyncHostResult<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(last_native_error())
    }
}

fn check_pthread_call(result: i32) -> AsyncHostResult<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(AsyncHostError::Native(result))
    }
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(last_native_errno())
}

#[cfg(target_os = "linux")]
fn last_native_errno() -> i32 {
    unsafe { *libc::__errno_location() }
}

#[cfg(target_os = "macos")]
fn last_native_errno() -> i32 {
    unsafe { *libc::__error() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SignalSendError, signal_channel};
    use std::os::fd::RawFd;
    use std::time::{Duration, Instant};

    fn close(fd: RawFd) {
        assert_eq!(unsafe { libc::close(fd) }, 0);
    }

    #[test]
    fn virtual_waiter_delivers_only_to_its_run_and_coalesces_signals() {
        let (first_sender, first_receiver) = signal_channel();
        let (second_sender, second_receiver) = signal_channel();
        first_receiver.configure(&[libc::SIGINT, libc::SIGTERM], &[libc::SIGINT]);
        second_receiver.configure(
            &[libc::SIGINT, libc::SIGTERM],
            &[libc::SIGINT, libc::SIGTERM],
        );

        let (first_notifier, first_fd) = ThreadPoolCompletionNotifier::new().unwrap();
        let first_notifier = Arc::new(first_notifier);
        let mut first_job = make_sigwait_job(
            &first_receiver,
            &[libc::SIGINT, libc::SIGTERM],
            Arc::clone(&first_notifier),
        )
        .unwrap();
        assert_eq!(
            make_sigwait_job(
                &first_receiver,
                &[libc::SIGINT],
                Arc::clone(&first_notifier),
            )
            .unwrap_err(),
            AsyncHostError::Inval
        );

        let (second_notifier, second_fd) = ThreadPoolCompletionNotifier::new().unwrap();
        let second_notifier = Arc::new(second_notifier);
        let mut second_job = make_sigwait_job(
            &second_receiver,
            &[libc::SIGINT],
            Arc::clone(&second_notifier),
        )
        .unwrap();

        // Signals may become pending after the virtual waiter is installed but
        // before its Worker begins running it.
        assert_eq!(first_sender.send(libc::SIGINT), Ok(true));
        assert_eq!(first_sender.send(libc::SIGINT), Ok(true));
        assert_eq!(second_sender.send(libc::SIGTERM), Ok(false));
        let first_thread = std::thread::spawn(move || first_job.run());
        let second_thread = std::thread::spawn(move || second_job.run());

        let deadline = Instant::now() + Duration::from_secs(1);
        let mut completions = [0; 8];
        let received = loop {
            let received = first_notifier.fetch(&mut completions).unwrap();
            if received != 0 {
                break received;
            }
            assert!(
                Instant::now() < deadline,
                "signal completion was not delivered"
            );
            std::thread::yield_now();
        };
        assert_eq!(received, 4);
        assert_eq!(
            i32::from_ne_bytes(completions[..4].try_into().unwrap()),
            ((libc::SIGINT as u32) | (1_u32 << 31)) as i32
        );
        assert_eq!(second_notifier.fetch(&mut [0; 4]).unwrap(), 0);

        drop(first_receiver);
        drop(second_receiver);
        assert_eq!(first_thread.join().unwrap(), Ok(0));
        assert_eq!(second_thread.join().unwrap(), Ok(0));

        drop(first_notifier);
        drop(second_notifier);
        close(first_fd);
        close(second_fd);

        assert_eq!(
            first_sender.send(libc::SIGINT),
            Err(SignalSendError::Disconnected)
        );
    }

    #[test]
    fn virtual_waiter_releases_its_run_target_when_dropped() {
        let (_sender, receiver) = signal_channel();
        receiver.configure(&[libc::SIGINT], &[libc::SIGINT]);
        let (notifier, notifier_fd) = ThreadPoolCompletionNotifier::new().unwrap();
        let notifier = Arc::new(notifier);

        let waiter = make_sigwait_job(&receiver, &[libc::SIGINT], Arc::clone(&notifier)).unwrap();
        assert_eq!(
            make_sigwait_job(&receiver, &[libc::SIGINT], Arc::clone(&notifier)).unwrap_err(),
            AsyncHostError::Inval
        );
        drop(waiter);

        let replacement = make_sigwait_job(&receiver, &[libc::SIGINT], Arc::clone(&notifier));
        assert!(replacement.is_ok());

        drop(replacement);
        drop(notifier);
        close(notifier_fd);
    }

    #[test]
    fn virtual_waiter_preserves_signal_then_cancellation_before_it_runs() {
        let (sender, receiver) = signal_channel();
        receiver.configure(&[libc::SIGINT], &[libc::SIGINT]);
        let (notifier, notifier_fd) = ThreadPoolCompletionNotifier::new().unwrap();
        let notifier = Arc::new(notifier);
        let mut waiter =
            make_sigwait_job(&receiver, &[libc::SIGINT], Arc::clone(&notifier)).unwrap();

        assert_eq!(sender.send(libc::SIGINT), Ok(true));
        assert_eq!(waiter.cancellation_override().cancel(), Ok(0));
        assert_eq!(sender.send(libc::SIGINT), Ok(false));
        assert_eq!(waiter.run(), Ok(0));

        let mut completion = [0; 4];
        assert_eq!(notifier.fetch(&mut completion).unwrap(), 4);
        assert_eq!(
            i32::from_ne_bytes(completion),
            ((libc::SIGINT as u32) | (1_u32 << 31)) as i32
        );

        drop(waiter);
        drop(notifier);
        close(notifier_fd);
    }
}
