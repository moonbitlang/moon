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
