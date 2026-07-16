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
#[cfg(windows)]
use crate::async_sys::internal::event_loop::poll::{self, CompletionPort};
use crate::async_sys::ported_fns;

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/signal.c",
        original = "moonbitlang_async_set_global_cancellation_signals"
    )]
    #[cfg(unix)]
    pub(crate) fn set_global_cancellation_signals(
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
        check_pthread_call(unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, &set, std::ptr::null_mut()) })
    }

    #[ported(
        source = "src/internal/event_loop/signal.c",
        original = "moonbitlang_async_set_global_cancellation_signals"
    )]
    #[cfg(windows)]
    pub(crate) fn set_global_cancellation_signals(
        _all_signals: &[i32],
        signals: &[i32],
    ) -> AsyncHostResult<()> {
        let mut mask = 0;
        for signal in signals
            .iter()
            .copied()
            .filter(|signal| (0..i32::BITS as i32).contains(signal))
        {
            mask |= 1_i32 << signal;
        }
        INTERESTED_CONSOLE_CTRL_EVENT.store(mask, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    #[ported(
        source = "src/internal/event_loop/signal.c",
        original = "moonbitlang_async_set_console_control_handler"
    )]
    #[cfg(windows)]
    pub(crate) fn set_console_control_handler(
        add: bool,
        completion_target: Option<(CompletionPort, usize)>,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

        if add {
            let completion_target = completion_target.ok_or(AsyncHostError::Badf)?;
            *CONSOLE_COMPLETION_TARGET.lock().unwrap() = Some(completion_target);
        }
        if unsafe { SetConsoleCtrlHandler(Some(console_control_handler), i32::from(add)) } == 0 {
            let error = last_native_error();
            if add {
                *CONSOLE_COMPLETION_TARGET.lock().unwrap() = None;
            }
            return Err(error);
        }
        if !add {
            *CONSOLE_COMPLETION_TARGET.lock().unwrap() = None;
        }
        Ok(1)
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

#[cfg(windows)]
static INTERESTED_CONSOLE_CTRL_EVENT: std::sync::atomic::AtomicI32 =
    std::sync::atomic::AtomicI32::new(0);

#[cfg(windows)]
static CONSOLE_COMPLETION_TARGET: std::sync::Mutex<Option<(CompletionPort, usize)>> =
    std::sync::Mutex::new(None);

#[cfg(windows)]
unsafe extern "system" fn console_control_handler(ctrl_type: u32) -> i32 {
    let interested = INTERESTED_CONSOLE_CTRL_EVENT.load(std::sync::atomic::Ordering::Relaxed);
    if ctrl_type < i32::BITS && (interested & (1_i32 << ctrl_type)) != 0 {
        let target = CONSOLE_COMPLETION_TARGET.lock().unwrap().clone();
        if let Some((completion_port, generation)) = target {
            let _ = poll::post_thread_pool_completion(
                &completion_port,
                (ctrl_type | (1 << 31)) as i32,
                generation,
            );
            return 1;
        }
    }
    0
}

#[cfg(unix)]
fn empty_signal_set() -> AsyncHostResult<libc::sigset_t> {
    let mut set = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    check_signal_call(unsafe { libc::sigemptyset(&mut set) })?;
    Ok(set)
}

#[cfg(unix)]
fn current_signal_mask() -> AsyncHostResult<libc::sigset_t> {
    let mut set = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    check_pthread_call(unsafe {
        libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &mut set)
    })?;
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

#[cfg(windows)]
fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(unsafe { windows_sys::Win32::Foundation::GetLastError() as i32 })
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
