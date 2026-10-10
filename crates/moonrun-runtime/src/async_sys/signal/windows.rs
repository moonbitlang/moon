// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
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
