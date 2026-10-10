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
use crate::guest_memory::GuestMemory;

pub(super) fn column_name16(
    context: &mut ImportContext,
    statement: u64,
    column: i32,
    output: u32,
    capacity: u32,
) -> SqliteResult<i32> {
    context.with_utf16_output(output, capacity, |host, output| {
        Ok(host.copy_column_name16(statement, column, output)?)
    })
}

pub(super) fn column_text16(
    context: &mut ImportContext,
    statement: u64,
    column: i32,
    output: u32,
    capacity: u32,
) -> SqliteResult<u32> {
    context.with_utf16_output(output, capacity, |host, output| {
        Ok(host.copy_column_text16(statement, column, output)?)
    })
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
