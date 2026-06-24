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

use crate::async_host::{AsyncHostError, write_u16};
use crate::async_sys::os_error::stub;

use super::context::ImportContext;
use super::provenance::ported_imports;

ported_imports! {
#[ported(source = "src/os_error/stub.c")]
pub(super) fn get_errno(context: &mut ImportContext) -> i32 {
    stub::get_errno(context.host)
}

#[ported(source = "src/os_error/stub.c")]
pub(super) fn is_nonblocking_io_error(_context: &mut ImportContext, errno: i32) -> i32 {
    if stub::is_nonblocking_io_error(errno) {
        1
    } else {
        0
    }
}

#[ported(source = "src/os_error/stub.c")]
pub(super) fn is_eintr(_context: &mut ImportContext, errno: i32) -> i32 {
    if stub::is_eintr(errno) { 1 } else { 0 }
}

#[ported(source = "src/os_error/stub.c")]
pub(super) fn is_enoent(_context: &mut ImportContext, errno: i32) -> i32 {
    if stub::is_enoent(errno) { 1 } else { 0 }
}

#[ported(source = "src/os_error/stub.c")]
pub(super) fn is_eexist(_context: &mut ImportContext, errno: i32) -> i32 {
    if stub::is_eexist(errno) { 1 } else { 0 }
}

#[ported(source = "src/os_error/stub.c")]
pub(super) fn is_eacces(_context: &mut ImportContext, errno: i32) -> i32 {
    if stub::is_eacces(errno) { 1 } else { 0 }
}

#[ported(source = "src/os_error/stub.c")]
pub(super) fn is_econnrefused(_context: &mut ImportContext, errno: i32) -> i32 {
    if stub::is_econnrefused(errno) { 1 } else { 0 }
}

#[ported(source = "src/os_error/stub.c")]
pub(super) fn is_error_notify_enum_dir(_context: &mut ImportContext, errno: i32) -> i32 {
    if stub::is_error_notify_enum_dir(errno) {
        1
    } else {
        0
    }
}

pub(super) fn errno_to_string_len(context: &mut ImportContext, errno: i32) -> i32 {
    match i32::try_from(errno_to_string_utf16(errno).len()).map_err(|_| AsyncHostError::Fault) {
        Ok(len) => len,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(source = "src/os_error/stub.c")]
pub(super) fn errno_to_string(context: &mut ImportContext, errno: i32, ptr: i32, len: i32) -> i32 {
    let result = (|| {
        let units = errno_to_string_utf16(errno);
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        if len != units.len() {
            return Err(AsyncHostError::Inval);
        }

        context.with_memory_mut(|memory| write_u16(memory, ptr, &units))
    })();

    match result {
        Ok(()) => 0,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(source = "src/os_error/stub.c")]
pub(super) fn get_enotdir(_context: &mut ImportContext) -> i32 {
    stub::get_enotdir()
}

fn errno_to_string_utf16(errno: i32) -> Vec<u16> {
    stub::errno_to_string(errno).encode_utf16().collect()
}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errno_to_string_utf16_returns_message_units() {
        assert!(!errno_to_string_utf16(stub::get_enotdir()).is_empty());
    }
}
