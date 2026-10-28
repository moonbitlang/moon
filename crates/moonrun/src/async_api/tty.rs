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

//! The controlling terminal, following `src/tty/tty.c` on async's tty branch.
//! Failures return `-1` and set errno, as in the native runtime.

use super::context::ImportContext;
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::guest_memory::GuestMemory;

fn errno_result(context: &mut ImportContext<'_>, result: AsyncHostResult<()>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

/// Fill `dst` with `[input, wakeup, control, output]` Handles.
pub(super) fn open(context: &mut ImportContext<'_>, dst: u32, len: u32) -> i32 {
    let result = (|| {
        if len < 4 {
            return Err(AsyncHostError::Fault);
        }
        // Validate the destination before opening anything.
        context.with_memory_mut(|memory| Ok(memory.read_exact_mut(dst, 32).map(|_| ())?))?;
        let handles = context.host.tty_open()?;
        context.with_memory_mut(|memory| {
            for (index, handle) in handles.into_iter().enumerate() {
                let offset = dst
                    .checked_add(8 * index as u32)
                    .ok_or(AsyncHostError::Fault)?;
                memory.write_u64_le(offset, handle)?;
            }
            Ok(())
        })
    })();
    errno_result(context, result)
}

/// The size of the state `enter_raw` writes, which the guest allocates.
pub(super) fn state_size(_context: &mut ImportContext<'_>) -> i32 {
    crate::async_sys::tty::state_size() as i32
}

/// Enter raw mode and write the old state, `state_size` opaque bytes, at
/// `dst`.
pub(super) fn enter_raw(
    context: &mut ImportContext<'_>,
    input: u64,
    output: u64,
    _flags: i32,
    dst: u32,
) -> i32 {
    let len = crate::async_sys::tty::state_size() as u32;
    let result = (|| {
        // Validate the destination before changing the terminal.
        context.with_memory_mut(|memory| Ok(memory.read_exact_mut(dst, len).map(|_| ())?))?;
        let state = context.host.tty_enter_raw(input, output)?;
        context.with_memory_mut(|memory| {
            memory.read_exact_mut(dst, len)?.copy_from_slice(&state);
            Ok(())
        })
    })();
    errno_result(context, result)
}

/// Restore the state `enter_raw` wrote, read back from `src`.
pub(super) fn leave_raw(context: &mut ImportContext<'_>, input: u64, output: u64, src: u32) -> i32 {
    let len = crate::async_sys::tty::state_size() as u32;
    let result = (|| {
        let state = context.with_memory_mut(|memory| Ok(memory.read_exact(src, len)?.to_vec()))?;
        context.host.tty_leave_raw(input, output, &state)
    })();
    errno_result(context, result)
}

/// Return `(rows << 32) | cols`.
pub(super) fn size(context: &mut ImportContext<'_>, output: u64) -> i64 {
    match context.host.tty_size(output) {
        Ok(size) => size,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}
