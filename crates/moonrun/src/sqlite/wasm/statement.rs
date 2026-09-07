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

pub(super) fn prepare16_v2(
    context: &mut ImportContext,
    database: u64,
    sql_pointer: u32,
    sql_offset: i32,
    sql_length: i32,
    statement_out: u32,
    tail_out: u32,
) -> SqliteResult<i32> {
    let sql_offset = u32::try_from(sql_offset).map_err(|_| SqliteError::Fault)?;
    let sql_length = u32::try_from(sql_length).map_err(|_| SqliteError::Fault)?;
    context.validate_write(statement_out, size_of::<u64>())?;
    if tail_out != 0 {
        context.validate_write(tail_out, size_of::<u32>())?;
    }
    context.write_u64(statement_out, null_handle())?;
    if tail_out != 0 {
        context.write_u32(tail_out, sql_offset)?;
    }

    let sql = context.read_utf16_view(sql_pointer, sql_offset, sql_length)?;

    // No guest callback can grow memory while the synchronous Host call
    // borrows these SQL bytes from Guest Memory.
    let outcome = context.host.prepare16_v2(database, sql)?;

    if tail_out != 0 {
        let guest_tail = sql_offset
            .checked_add(outcome.tail_offset)
            .ok_or(SqliteError::Overflow)?;
        context.write_u32(tail_out, guest_tail)?;
    }
    context.write_u64(statement_out, outcome.statement.unwrap_or_else(null_handle))?;
    Ok(outcome.code)
}
