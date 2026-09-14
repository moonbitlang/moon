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

use crate::async_host::AsyncHostError;
use crate::async_sys::signal;
use crate::guest_memory::GuestMemory;

use super::context::ImportContext;
use super::provenance::ported_imports;

ported_imports! {
#[ported(source = "src/internal/event_loop/signal.c")]
#[cfg(unix)]
pub(super) fn start_signal_handler(context: &mut ImportContext<'_, '_>) -> crate::async_host::AsyncHostResult<()> {
    context.host.start_signal_handler()
}

#[ported(source = "src/internal/event_loop/signal.c")]
#[cfg(unix)]
pub(super) fn terminate_signal_handler(context: &mut ImportContext<'_, '_>) {
    context.host.terminate_signal_handler();
}

pub(super) fn get_signal_by_index(
    _context: &mut ImportContext<'_, '_>,
    index: u32,
) -> i32 {
    signal::get_signal_by_index(index)
}

#[ported(source = "src/internal/event_loop/signal.c")]
#[cfg(any(unix, windows))]
pub(super) fn set_global_cancellation_signals(
    context: &mut ImportContext<'_, '_>,
    all_signals: u32,
    all_signals_len: u32,
    signals: u32,
    signals_len: u32,
) -> crate::async_host::AsyncHostResult<()> {
    // "global" is the upstream guest ABI name. Moonrun scopes the resulting
    // registration to this Import Context's Run.
    let all_signals = context.with_memory_mut(|memory| read_i32_array(memory, all_signals, all_signals_len))?;
    let signals = context.with_memory_mut(|memory| read_i32_array(memory, signals, signals_len))?;
    context
        .host
        .set_cancellation_signals(&all_signals, &signals)
}

#[ported(source = "src/internal/event_loop/signal.c")]
#[cfg(windows)]
pub(super) fn set_console_control_handler(
    context: &mut ImportContext<'_, '_>,
    add: i32,
) -> crate::async_host::AsyncHostResult<i32> {
    context.host.set_signal_delivery_enabled(add != 0)
}
}

#[cfg(any(unix, windows))]
fn read_i32_array(
    memory: &(impl GuestMemory + ?Sized),
    offset: u32,
    len: u32,
) -> crate::async_host::AsyncHostResult<Vec<i32>> {
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let byte_len = len
        .checked_mul(std::mem::size_of::<i32>())
        .ok_or(AsyncHostError::Fault)?;
    let bytes = memory.read_exact(
        offset,
        u32::try_from(byte_len).map_err(|_| AsyncHostError::Fault)?,
    )?;
    Ok(bytes
        .chunks_exact(std::mem::size_of::<i32>())
        .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()))
        .collect())
}
