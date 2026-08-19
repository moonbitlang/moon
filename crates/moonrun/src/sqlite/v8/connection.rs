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
use crate::host::null_handle;

pub(super) fn open_v2(
    context: &mut ImportContext,
    filename: u32,
    filename_length: i32,
    database_out: u32,
    flags: i32,
    vfs: u64,
) -> SqliteResult<i32> {
    let filename_length = u32::try_from(filename_length).map_err(|_| SqliteError::Fault)?;
    context.validate_write(database_out, size_of::<u64>())?;
    context.write_u64(database_out, null_handle())?;

    let filename = context.read_utf8_c_string(filename, filename_length)?;
    let outcome = context.host.open_v2(filename, flags, vfs);
    context.write_u64(database_out, outcome.database.unwrap_or_else(null_handle))?;
    Ok(outcome.code)
}

/// Copy the current connection error into Guest Memory as UTF-16LE.
///
/// Capacity and the return value are UTF-16 content code units. SQLite's
/// trailing NUL is not copied. If the current message does not fit, the output
/// is left unchanged and its current content length is returned so the caller
/// can retry.
pub(super) fn errmsg16(
    context: &mut ImportContext,
    database: u64,
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
    // SAFETY: every bit pattern is a valid `u16`. The prefix/suffix check
    // rejects a runtime memory backing that does not satisfy this ABI's
    // alignment requirement.
    let (prefix, output, suffix) = unsafe { bytes.align_to_mut::<u16>() };
    if !prefix.is_empty() || !suffix.is_empty() {
        return Err(SqliteError::Fault);
    }
    Ok(host.copy_errmsg16(database, output)?)
}
