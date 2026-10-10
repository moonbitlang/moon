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
