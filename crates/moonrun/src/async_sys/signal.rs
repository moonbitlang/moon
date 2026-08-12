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
use crate::async_sys::ported_fns;
use crate::run_termination::RunTermination;

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/signal.c",
        original = "moonbitlang_async_set_global_cancellation_signals"
    )]
    pub(crate) fn set_global_cancellation_signals(
        all_signals: &[i32],
        signals: &[i32],
    ) -> Vec<i32> {
        // Native execution updates the process thread's signal mask here. A
        // Wasm instance instead returns the selected set to its controller so
        // the embedding process keeps ownership of OS signal disposition.
        signals
            .iter()
            .copied()
            .filter(|signal| *signal >= 0 && all_signals.contains(signal))
            .collect()
    }

    #[ported(
        source = "src/internal/event_loop/signal.c",
        original = "moonbitlang_async_set_console_control_handler"
    )]
    #[cfg(windows)]
    pub(crate) fn set_console_control_handler(add: bool) -> bool {
        // The CLI or another embedding adapter owns SetConsoleCtrlHandler.
        // The instance retains only the native add/remove lifecycle.
        add
    }

    #[ported(
        source = "src/internal/event_loop/signal.c",
        original = "moonbitlang_async_terminate_process_by_signal"
    )]
    pub(crate) fn terminate_process_by_signal(signal: i32) -> RunTermination {
        // The outer adapter applies this outcome only after instance cleanup.
        RunTermination::KilledBySignal(signal)
    }
}

pub(crate) fn get_signal_by_index(index: i32) -> i32 {
    match index {
        0 => signal_int(),
        1 => signal_term(),
        2 => signal_hup(),
        3 => signal_break(),
        _ => -1,
    }
}

#[cfg(unix)]
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

#[cfg(unix)]
pub(crate) fn restore_thread_pool_signal_mask(old: &libc::sigset_t) -> AsyncHostResult<()> {
    check_pthread_call(unsafe {
        libc::pthread_sigmask(libc::SIG_SETMASK, old, std::ptr::null_mut())
    })
}

#[cfg(unix)]
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

#[cfg(unix)]
fn signal_int() -> i32 {
    libc::SIGINT
}

#[cfg(windows)]
fn signal_int() -> i32 {
    windows_sys::Win32::System::Console::CTRL_C_EVENT as i32
}

#[cfg(unix)]
fn signal_term() -> i32 {
    libc::SIGTERM
}

#[cfg(windows)]
fn signal_term() -> i32 {
    -1
}

#[cfg(unix)]
fn signal_hup() -> i32 {
    libc::SIGHUP
}

#[cfg(windows)]
fn signal_hup() -> i32 {
    windows_sys::Win32::System::Console::CTRL_CLOSE_EVENT as i32
}

#[cfg(unix)]
fn signal_break() -> i32 {
    -1
}

#[cfg(windows)]
fn signal_break() -> i32 {
    windows_sys::Win32::System::Console::CTRL_BREAK_EVENT as i32
}

#[cfg(unix)]
fn empty_signal_set() -> AsyncHostResult<libc::sigset_t> {
    let mut set = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    check_signal_call(unsafe { libc::sigemptyset(&mut set) })?;
    Ok(set)
}

#[cfg(unix)]
fn check_signal_call(result: i32) -> AsyncHostResult<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(last_native_error())
    }
}

#[cfg(unix)]
fn check_pthread_call(result: i32) -> AsyncHostResult<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(AsyncHostError::Native(result))
    }
}

#[cfg(unix)]
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
