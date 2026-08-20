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

use super::context::{ImportContext, SqliteError, SqliteResult};
use crate::guest_memory::GuestMemory;

pub(super) fn column_text16(
    context: &mut ImportContext,
    statement: u64,
    column: i32,
    output: u32,
    capacity: u32,
) -> SqliteResult<u32> {
    let byte_capacity = capacity.checked_mul(2).ok_or(SqliteError::Overflow)?;
    let (host, memory) = context.host_and_memory();
    let bytes = if capacity == 0 {
        &mut []
    } else {
        if output == 0 {
            return Err(SqliteError::Fault);
        }
        memory.read_exact_mut(output, byte_capacity)?
    };
    // SAFETY: every bit pattern is a valid `u16`; the alignment check rejects
    // a Guest Memory range that does not satisfy this ABI's promise.
    let (prefix, output, suffix) = unsafe { bytes.align_to_mut::<u16>() };
    if !prefix.is_empty() || !suffix.is_empty() {
        return Err(SqliteError::Fault);
    }
    Ok(host.copy_column_text16(statement, column, output)?)
}

pub(super) fn column_blob(
    context: &mut ImportContext,
    statement: u64,
    column: i32,
    output: u32,
    capacity: u32,
) -> SqliteResult<u32> {
    let (host, memory) = context.host_and_memory();
    let output = if capacity == 0 {
        &mut []
    } else {
        if output == 0 {
            return Err(SqliteError::Fault);
        }
        memory.read_exact_mut(output, capacity)?
    };
    Ok(host.copy_column_blob(statement, column, output)?)
}
