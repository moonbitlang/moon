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

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::ported_fns;

ported_fns! {
    #[ported(
        source = "src/internal/os_string/stub.c",
        original = "moonbitlang_async_c_buffer_as_string"
    )]
    #[allow(dead_code)]
    pub(crate) fn c_buffer_as_string(buffer: &[u8], len: i32) -> AsyncHostResult<String> {
        let bytes = if len == -1 {
            let nul = buffer
                .chunks_exact(2)
                .position(|unit| unit == [0, 0])
                .ok_or(AsyncHostError::Fault)?;
            &buffer[..nul * 2]
        } else {
            let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
            buffer.get(..len).ok_or(AsyncHostError::Fault)?
        };

        if bytes.len() % 2 != 0 {
            return Err(AsyncHostError::Inval);
        }

        let units = bytes
            .chunks_exact(2)
            .map(|bytes| u16::from_ne_bytes([bytes[0], bytes[1]]));
        String::from_utf16(&units.collect::<Vec<_>>()).map_err(|_| AsyncHostError::Inval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_buffer_as_string_decodes_explicit_byte_length() {
        let bytes = [b'a', 0, 0x34, 0xd8, 0x1e, 0xdd, 0, 0];

        assert_eq!(c_buffer_as_string(&bytes, 6).unwrap(), "a\u{1d11e}");
    }

    #[test]
    fn c_buffer_as_string_uses_nul_terminated_length() {
        let bytes = [b'o', 0, b'k', 0, 0, 0, b'x', 0];

        assert_eq!(c_buffer_as_string(&bytes, -1).unwrap(), "ok");
    }
}
