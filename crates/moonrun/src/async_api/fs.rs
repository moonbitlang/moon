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

use crate::async_host::{AsyncHostError, AsyncHostResult, write_u16};
use crate::async_sys::fs::dir;
use crate::async_sys::fs::stub;

use super::context::ImportContext;
use super::provenance::ported_imports;

ported_imports! {
pub(super) fn get_tmp_path_len(context: &mut ImportContext<'_, '_>) -> i32 {
    match tmp_path_utf16_units()
        .and_then(|units| i32::try_from(units.len()).map_err(|_| AsyncHostError::Fault))
    {
        Ok(len) => len,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(source = "src/fs/stub.c")]
pub(super) fn get_tmp_path(context: &mut ImportContext<'_, '_>, ptr: i32, len: i32) -> i32 {
    let result = (|| {
        let units = tmp_path_utf16_units()?;
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        if len != units.len() {
            return Err(AsyncHostError::Inval);
        }
        context.with_memory_mut(|memory| write_u16(memory, ptr, &units))
    })();
    zero_or_minus_one(context, result)
}

#[ported(source = "src/internal/fd_util/stub.c")]
pub(super) fn close_fd(context: &mut ImportContext<'_, '_>, fd: u64) -> i32 {
    zero_or_minus_one(context, context.host.close_fd(fd))
}

#[ported(source = "src/fs/dir.c")]
pub(super) fn dir_buffer_min_size(_context: &mut ImportContext<'_, '_>) -> i32 {
    dir::buffer_min_size()
}

#[ported(source = "src/fs/dir.c")]
pub(super) fn dir_entry_length(
    context: &mut ImportContext<'_, '_>,
    buf: u64,
    offset: i32,
) -> AsyncHostResult<i32> {
    context.host
        .with_c_buffer(buf, |buf| dir::entry_length(buf, 0, offset))
}

#[ported(source = "src/fs/dir.c")]
pub(super) fn dir_entry_name_len(
    context: &mut ImportContext<'_, '_>,
    buf: u64,
    offset: i32,
) -> AsyncHostResult<i32> {
    context.host
        .with_c_buffer(buf, |buf| dir::entry_name_len(buf, 0, offset))
}

#[ported(source = "src/fs/dir.c")]
pub(super) fn dir_entry_name_offset(
    context: &mut ImportContext<'_, '_>,
    buf: u64,
    offset: i32,
) -> AsyncHostResult<i32> {
    context.host
        .with_c_buffer(buf, |buf| dir::entry_name_offset(buf, 0, offset))
}

#[ported(source = "src/fs/dir.c")]
pub(super) fn dir_entry_is_dir(
    context: &mut ImportContext<'_, '_>,
    buf: u64,
    offset: i32,
) -> AsyncHostResult<i32> {
    context.host
        .with_c_buffer(buf, |buf| dir::entry_is_dir(buf, 0, offset))
}

#[ported(source = "src/fs/dir.c")]
pub(super) fn dir_entry_is_hidden(
    context: &mut ImportContext<'_, '_>,
    buf: u64,
    offset: i32,
) -> AsyncHostResult<i32> {
    context.host
        .with_c_buffer(buf, |buf| dir::entry_is_hidden(buf, 0, offset))
        .map(|value| if value { 1 } else { 0 })
}

#[ported(source = "src/fs/dir.c")]
pub(super) fn dir_entry_file_id(
    context: &mut ImportContext<'_, '_>,
    buf: u64,
    offset: i32,
) -> AsyncHostResult<u64> {
    context.host
        .with_c_buffer(buf, |buf| dir::entry_file_id(buf, 0, offset))
}

fn tmp_path_utf16_units() -> AsyncHostResult<Vec<u16>> {
    os_string_to_utf16_units(stub::get_tmp_path()?)
}

#[cfg(unix)]
fn os_string_to_utf16_units(path: std::ffi::OsString) -> AsyncHostResult<Vec<u16>> {
    use std::os::unix::ffi::OsStringExt;

    let path = String::from_utf8(path.into_vec()).map_err(|_| AsyncHostError::Inval)?;
    Ok(path.encode_utf16().collect())
}

#[cfg(windows)]
fn os_string_to_utf16_units(path: std::ffi::OsString) -> AsyncHostResult<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;

    Ok(path.as_os_str().encode_wide().collect())
}

#[ported(source = "src/fs/stub.c")]
pub(super) fn errno_is_lock_violation(_context: &mut ImportContext<'_, '_>, errno: i32) -> i32 {
    if stub::errno_is_lock_violation(errno) {
        1
    } else {
        0
    }
}

#[ported(source = "src/fs/stub.c")]
pub(super) fn try_lock_file(context: &mut ImportContext<'_, '_>, fd: u64, exclusive: i32) -> i32 {
    zero_or_minus_one(context, context.host.try_lock_file(fd, exclusive != 0))
}

#[ported(source = "src/fs/stub.c")]
pub(super) fn unlock_file(context: &mut ImportContext<'_, '_>, fd: u64) -> i32 {
    zero_or_minus_one(context, context.host.unlock_file(fd))
}

fn zero_or_minus_one(context: &ImportContext<'_, '_>, result: AsyncHostResult<()>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn tmp_path_encodes_unix_path_as_utf16_units() {
        let path = std::ffi::OsString::from("/tmp/\u{6587}");

        let units = os_string_to_utf16_units(path).unwrap();

        assert_eq!(units, "/tmp/\u{6587}".encode_utf16().collect::<Vec<_>>());
    }

    #[cfg(unix)]
    #[test]
    fn tmp_path_rejects_non_utf8_unix_os_string() {
        use std::os::unix::ffi::OsStringExt;

        let path = std::ffi::OsString::from_vec(b"/tmp/\xff".to_vec());

        assert_eq!(os_string_to_utf16_units(path), Err(AsyncHostError::Inval));
    }

    #[cfg(windows)]
    #[test]
    fn tmp_path_preserves_windows_wide_units() {
        let path = std::ffi::OsString::from("A\u{10000}");

        let units = os_string_to_utf16_units(path).unwrap();

        assert_eq!(units, vec![0x0041, 0xd800, 0xdc00]);
    }
}
