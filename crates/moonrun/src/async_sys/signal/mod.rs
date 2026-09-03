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

//! Platform signal values and Unix worker signal-mask management.
//!
//! Process signal capture belongs to the CLI. Guest signal registration and
//! completion delivery belong to one Run.

use crate::async_host::AsyncHostResult;
#[cfg(windows)]
use crate::async_sys::internal::event_loop::poll::CompletionPort;
use crate::async_sys::ported_fns;
use crate::run_signal::SignalReceiver;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
use windows as platform;

#[cfg(unix)]
pub(crate) use unix::{
    SigwaitJob, SigwaitTarget, init_thread_pool_signal_mask, make_sigwait_job,
    restore_thread_pool_signal_mask, set_worker_thread_signal_mask,
};

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/signal.c",
        original = "moonbitlang_async_set_global_cancellation_signals"
    )]
    pub(crate) fn set_global_cancellation_signals(
        receiver: &SignalReceiver,
        all_signals: &[i32],
        signals: &[i32],
    ) -> AsyncHostResult<()> {
        platform::set_global_cancellation_signals(receiver, all_signals, signals)
    }

    #[ported(
        source = "src/internal/event_loop/signal.c",
        original = "moonbitlang_async_set_console_control_handler"
    )]
    #[cfg(windows)]
    pub(crate) fn set_console_control_handler(
        receiver: &SignalReceiver,
        add: bool,
        completion_target: Option<CompletionPort>,
    ) -> AsyncHostResult<i32> {
        windows::set_console_control_handler(receiver, add, completion_target)
    }
}

pub(crate) fn get_signal_by_index(index: u32) -> i32 {
    match index {
        0 => platform::signal_int(),
        1 => platform::signal_term(),
        2 => platform::signal_hup(),
        3 => platform::signal_break(),
        _ => -1,
    }
}
