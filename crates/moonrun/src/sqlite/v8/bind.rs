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

pub(super) fn bind_text16(
    context: &mut ImportContext,
    statement: u64,
    index: i32,
    value: u32,
    value_offset: i32,
    value_length: i32,
) -> SqliteResult<i32> {
    let value_offset = u32::try_from(value_offset).map_err(|_| SqliteError::Fault)?;
    let value_length = u32::try_from(value_length).map_err(|_| SqliteError::Fault)?;
    let value = context.read_utf16_view(value, value_offset, value_length)?;
    Ok(context.host.bind_text16(statement, index, value)?)
}

pub(super) fn bind_blob(
    context: &mut ImportContext,
    statement: u64,
    index: i32,
    value: u32,
    value_offset: i32,
    value_length: i32,
) -> SqliteResult<i32> {
    let value_offset = u32::try_from(value_offset).map_err(|_| SqliteError::Fault)?;
    let value_length = u32::try_from(value_length).map_err(|_| SqliteError::Fault)?;
    let value = context.read_bytes_view(value, value_offset, value_length)?;
    Ok(context.host.bind_blob(statement, index, value)?)
}
