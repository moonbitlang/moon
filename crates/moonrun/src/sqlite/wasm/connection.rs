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
use crate::runtime::null_handle;

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
    let outcome = context.host.open_v2(&filename, flags, vfs);
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
    context.with_utf16_output(output, capacity, |host, output| {
        Ok(host.copy_errmsg16(database, output)?)
    })
}
