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

use std::sync::Arc;

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier;
use crate::async_sys::ported_fns;

#[derive(Debug)]
pub(crate) struct SigwaitJob {
    signals: Vec<i32>,
    notifier: Arc<ThreadPoolCompletionNotifier>,
}

impl SigwaitJob {
    pub(crate) fn new(signals: Vec<i32>, notifier: Arc<ThreadPoolCompletionNotifier>) -> Self {
        Self { signals, notifier }
    }

    pub(crate) fn run(&mut self) -> AsyncHostResult<i64> {
        run_sigwait_job(&self.signals, &self.notifier)
    }
}

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "sigwait_job_worker"
    )]
    fn run_sigwait_job(
        signals: &[i32],
        notifier: &ThreadPoolCompletionNotifier,
    ) -> AsyncHostResult<i64> {
        let mut set = empty_signal_set()?;
        for signal in signals.iter().copied().filter(|signal| *signal > 0) {
            check_signal_call(unsafe { libc::sigaddset(&mut set, signal) })?;
        }
        check_signal_call(unsafe { libc::sigaddset(&mut set, libc::SIGUSR2) })?;

        // The Thread Pool interrupts blocking workers with SIGUSR2. This Job
        // consumes that executor signal because sigwait would otherwise keep
        // the worker parked after cancellation.
        let mut all_signals = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        check_signal_call(unsafe { libc::sigfillset(&mut all_signals) })?;
        let _mask_guard = SignalMaskGuard::replace(&all_signals)?;

        loop {
            let mut signal = 0;
            let error = unsafe { libc::sigwait(&set, &mut signal) };
            if error > 0 {
                break Err(AsyncHostError::Native(error));
            }
            if signal == libc::SIGUSR2 {
                break Ok(0);
            }

            let completion_id = ((signal as u32) | (1u32 << 31)) as i32;
            notifier.notify(completion_id)?;
        }
    }
}

struct SignalMaskGuard {
    old: libc::sigset_t,
}

impl SignalMaskGuard {
    fn replace(set: &libc::sigset_t) -> AsyncHostResult<Self> {
        let mut old = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        check_pthread_call(unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, set, &mut old) })?;
        Ok(Self { old })
    }
}

impl Drop for SignalMaskGuard {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.old, std::ptr::null_mut());
        }
    }
}

pub(super) fn set_global_cancellation_signals(
    all_signals: &[i32],
    signals: &[i32],
) -> AsyncHostResult<()> {
    let mut set = current_signal_mask()?;
    for signal in all_signals.iter().copied().filter(|signal| *signal >= 0) {
        check_signal_call(unsafe { libc::sigdelset(&mut set, signal) })?;
    }
    for signal in signals.iter().copied().filter(|signal| *signal >= 0) {
        check_signal_call(unsafe { libc::sigaddset(&mut set, signal) })?;
    }
    check_pthread_call(unsafe {
        libc::pthread_sigmask(libc::SIG_SETMASK, &set, std::ptr::null_mut())
    })
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

fn current_signal_mask() -> AsyncHostResult<libc::sigset_t> {
    let mut set = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    check_pthread_call(unsafe {
        libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &mut set)
    })?;
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
    #[test]
    fn sigwait_job_references_native_worker_symbol() {
        let async_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../third_party/moonbitlang_async");
        for symbol in super::PORTED_SYMBOLS {
            let source_path = async_root.join(symbol.source);
            let contents = std::fs::read_to_string(&source_path)
                .unwrap_or_else(|error| panic!("failed to read {:?}: {error}", source_path));
            assert!(
                contents.contains(symbol.native_symbol),
                "{:?} does not contain native worker symbol {} for {}::{}",
                source_path,
                symbol.native_symbol,
                symbol.rust_module,
                symbol.rust_symbol
            );
        }
    }
}
