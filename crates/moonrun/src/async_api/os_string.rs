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

use super::context::ImportContext;

pub(super) fn decode_len(context: &mut ImportContext, ptr: u64, len: i32) -> AsyncHostResult<i32> {
    let string = context
        .host
        .with_c_buffer(ptr, |buffer| decode_native_string(buffer, len))?;
    utf16_len(&string)
}

pub(super) fn decode(
    context: &mut ImportContext,
    ptr: u64,
    len: i32,
    out: i32,
    out_len: i32,
) -> AsyncHostResult<()> {
    let string = context
        .host
        .with_c_buffer(ptr, |buffer| decode_native_string(buffer, len))?;
    let units = string.encode_utf16().collect::<Vec<_>>();
    let actual_len = i32::try_from(units.len()).map_err(|_| AsyncHostError::Fault)?;
    if actual_len != out_len {
        return Err(AsyncHostError::Inval);
    }
    context.with_memory_mut(|memory| write_u16(memory, out, &units))
}

fn utf16_len(string: &str) -> AsyncHostResult<i32> {
    i32::try_from(string.encode_utf16().count()).map_err(|_| AsyncHostError::Fault)
}

fn decode_native_string(bytes: &[u8], len: i32) -> AsyncHostResult<String> {
    let bytes = if len == -1 {
        bytes
    } else {
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        bytes.get(..len).ok_or(AsyncHostError::Fault)?
    };
    decode_native_string_bytes(bytes)
}

#[cfg(unix)]
fn decode_native_string_bytes(bytes: &[u8]) -> AsyncHostResult<String> {
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(windows)]
fn decode_native_string_bytes(bytes: &[u8]) -> AsyncHostResult<String> {
    if !bytes.len().is_multiple_of(std::mem::size_of::<u16>()) {
        return Err(AsyncHostError::Inval);
    }
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]));
    Ok(std::char::decode_utf16(units)
        .map(|result| result.unwrap_or(std::char::REPLACEMENT_CHARACTER))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn unix_decode_native_string_uses_the_whole_owned_buffer() {
        assert_eq!(
            decode_native_string(b"abc\0def", -1),
            Ok("abc\0def".to_string())
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_decode_native_string_uses_the_whole_owned_buffer() {
        assert_eq!(
            decode_native_string(&[b'a', 0, b'b', 0, 0, 0], -1),
            Ok("ab\0".to_string())
        );
    }

    #[test]
    fn decoded_len_is_utf16_code_units() {
        assert_eq!(utf16_len("a\u{10000}"), Ok(3));
    }
}
