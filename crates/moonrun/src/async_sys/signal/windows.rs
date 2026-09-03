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
use crate::async_sys::internal::event_loop::poll::CompletionPort;
use crate::run_signal::SignalReceiver;

pub(super) fn set_global_cancellation_signals(
    receiver: &SignalReceiver,
    all_signals: &[i32],
    signals: &[i32],
) -> AsyncHostResult<()> {
    receiver.configure(all_signals, signals);
    Ok(())
}

pub(super) fn set_console_control_handler(
    receiver: &SignalReceiver,
    add: bool,
    completion_target: Option<CompletionPort>,
) -> AsyncHostResult<i32> {
    // The CLI owns the one process-global Windows callback. The guest's
    // set/unset operation instead selects whether that callback may forward
    // to this Run's completion port.
    if add {
        receiver.attach_target(completion_target.ok_or(AsyncHostError::Badf)?);
    } else {
        receiver.detach_target();
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
