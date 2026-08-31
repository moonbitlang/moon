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
use crate::async_sys::internal::event_loop::poll::{self, CompletionPort};

static INTERESTED_CONSOLE_CTRL_EVENT: std::sync::atomic::AtomicI32 =
    std::sync::atomic::AtomicI32::new(0);

static CONSOLE_COMPLETION_TARGET: std::sync::Mutex<Option<CompletionPort>> =
    std::sync::Mutex::new(None);

pub(super) fn set_global_cancellation_signals(
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

pub(super) fn set_console_control_handler(
    add: bool,
    completion_target: Option<CompletionPort>,
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

pub(super) fn signal_int() -> i32 {
    windows_sys::Win32::System::Console::CTRL_C_EVENT as i32
}

pub(super) fn signal_term() -> i32 {
    -1
}

pub(super) fn signal_hup() -> i32 {
    windows_sys::Win32::System::Console::CTRL_CLOSE_EVENT as i32
}

pub(super) fn signal_break() -> i32 {
    windows_sys::Win32::System::Console::CTRL_BREAK_EVENT as i32
}

unsafe extern "system" fn console_control_handler(ctrl_type: u32) -> i32 {
    let interested = INTERESTED_CONSOLE_CTRL_EVENT.load(std::sync::atomic::Ordering::Relaxed);
    if ctrl_type < i32::BITS && (interested & (1_i32 << ctrl_type)) != 0 {
        let target = CONSOLE_COMPLETION_TARGET.lock().unwrap().clone();
        if let Some(completion_port) = target {
            let _ =
                poll::post_thread_pool_completion(&completion_port, (ctrl_type | (1 << 31)) as i32);
            return 1;
        }
    }
    0
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(unsafe { windows_sys::Win32::Foundation::GetLastError() as i32 })
}
