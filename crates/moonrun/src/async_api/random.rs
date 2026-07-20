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

use rand::{RngCore, rngs::OsRng};

use crate::async_host::{AsyncHostError, GuestMemory};

use super::context::ImportContext;

pub(super) fn fill(context: &mut ImportContext<'_, '_>, buffer: i32, length: i32) -> i32 {
    let result = context.with_memory_mut(|memory| {
        let destination = memory.read_exact_mut(buffer, length)?;
        OsRng.try_fill_bytes(destination).map_err(|error| {
            error
                .raw_os_error()
                .map_or(AsyncHostError::Io, AsyncHostError::Native)
        })
    });
    match result {
        Ok(()) => 0,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}
