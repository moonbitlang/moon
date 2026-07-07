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
use crate::async_host::AsyncHostError;
use crate::async_host::GuestMemory;
use crate::async_sys::signal;

use super::context::ImportContext;
use super::provenance::ported_imports;

ported_imports! {
pub(super) fn get_signal_by_index(
    _context: &mut ImportContext<'_, '_>,
    index: i32,
) -> i32 {
    signal::get_signal_by_index(index)
}

#[ported(source = "src/internal/event_loop/signal.c")]
pub(super) fn get_signal_by_name(
    context: &mut ImportContext<'_, '_>,
    name: i32,
    name_len: i32,
) -> crate::async_host::AsyncHostResult<i32> {
    context.with_memory_mut(|memory| {
        let name = memory.read_exact(name, name_len)?;
        Ok(signal::get_signal_by_name(name))
    })
}

#[ported(source = "src/internal/event_loop/signal.c")]
#[cfg(unix)]
pub(super) fn set_global_cancellation_signals(
    context: &mut ImportContext<'_, '_>,
    all_signals: i32,
    signals: i32,
    all_signals_len: i32,
    signals_len: i32,
) -> crate::async_host::AsyncHostResult<()> {
    let all_signals = context.with_memory_mut(|memory| read_i32_array(memory, all_signals, all_signals_len))?;
    let signals = context.with_memory_mut(|memory| read_i32_array(memory, signals, signals_len))?;
    signal::set_global_cancellation_signals(&all_signals, &signals)
}
}

#[cfg(unix)]
fn read_i32_array(
    memory: &(impl GuestMemory + ?Sized),
    offset: i32,
    len: i32,
) -> crate::async_host::AsyncHostResult<Vec<i32>> {
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let byte_len = len
        .checked_mul(std::mem::size_of::<i32>())
        .ok_or(AsyncHostError::Fault)?;
    let bytes = memory.read_exact(
        offset,
        i32::try_from(byte_len).map_err(|_| AsyncHostError::Fault)?,
    )?;
    Ok(bytes
        .chunks_exact(std::mem::size_of::<i32>())
        .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()))
        .collect())
}
