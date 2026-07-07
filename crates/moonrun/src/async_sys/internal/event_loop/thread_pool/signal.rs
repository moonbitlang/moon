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

//! Signal-waiting job ported from
//! `moonbitlang/async/src/internal/event_loop/thread_pool.c`.

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier;
use crate::async_sys::ported_fns;

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "sigwait_job_worker"
    )]
    pub(super) fn run_sigwait_job(
        signals: &[i32],
        notifier: &ThreadPoolCompletionNotifier,
    ) -> AsyncHostResult<i64> {
        let mut set = empty_signal_set()?;
        for signal in signals.iter().copied().filter(|signal| *signal > 0) {
            check_signal_call(unsafe { libc::sigaddset(&mut set, signal) })?;
        }
        check_signal_call(unsafe { libc::sigaddset(&mut set, libc::SIGUSR2) })?;

        let mut old = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        check_pthread_call(unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &set, &mut old) })?;

        let result = loop {
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
        };

        let restore = check_pthread_call(unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &old, std::ptr::null_mut())
        });
        result.and(restore.map(|_| 0))
    }
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
    AsyncHostError::Native(
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}
