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
        source = "src/internal/c_buffer/stub.c",
        original = "moonbitlang_async_blit_to_c"
    )]
    #[allow(dead_code)]
    pub(crate) fn blit_to_c(dst: &mut [u8], src: &[u8], offset: i32, len: i32) -> AsyncHostResult<()> {
        let src = checked_range(src, offset, len)?;
        let dst = dst.get_mut(..src.len()).ok_or(AsyncHostError::Fault)?;
        dst.copy_from_slice(src);
        Ok(())
    }

    #[ported(
        source = "src/internal/c_buffer/stub.c",
        original = "moonbitlang_async_blit_from_c"
    )]
    #[allow(dead_code)]
    pub(crate) fn blit_from_c(src: &[u8], dst: &mut [u8], offset: i32, len: i32) -> AsyncHostResult<()> {
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let src = src.get(..len).ok_or(AsyncHostError::Fault)?;
        let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
        let end = offset.checked_add(len).ok_or(AsyncHostError::Fault)?;
        let dst = dst.get_mut(offset..end).ok_or(AsyncHostError::Fault)?;
        dst.copy_from_slice(src);
        Ok(())
    }

    #[ported(
        source = "src/internal/c_buffer/stub.c",
        original = "moonbitlang_async_c_buffer_get"
    )]
    #[allow(dead_code)]
    pub(crate) fn c_buffer_get(buf: &[u8], index: i32) -> AsyncHostResult<u8> {
        let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
        buf.get(index).copied().ok_or(AsyncHostError::Fault)
    }

    #[ported(
        source = "src/internal/c_buffer/stub.c",
        original = "moonbitlang_async_strlen"
    )]
    #[allow(dead_code)]
    pub(crate) fn strlen(buf: &[u8]) -> AsyncHostResult<i32> {
        let len = buf
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(AsyncHostError::Fault)?;
        i32::try_from(len).map_err(|_| AsyncHostError::Fault)
    }

    #[ported(
        source = "src/internal/c_buffer/stub.c",
        original = "moonbitlang_async_null_pointer"
    )]
    #[allow(dead_code)]
    pub(crate) fn null_pointer() -> i32 {
        0
    }

    #[ported(
        source = "src/internal/c_buffer/stub.c",
        original = "moonbitlang_async_pointer_is_null"
    )]
    #[allow(dead_code)]
    pub(crate) fn pointer_is_null(ptr: i32) -> bool {
        ptr == 0
    }
}

fn checked_range(src: &[u8], offset: i32, len: i32) -> AsyncHostResult<&[u8]> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let end = offset.checked_add(len).ok_or(AsyncHostError::Fault)?;
    src.get(offset..end).ok_or(AsyncHostError::Fault)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blit_to_c_copies_from_source_offset() {
        let mut dst = [0; 3];

        blit_to_c(&mut dst, b"abcdef", 2, 3).unwrap();

        assert_eq!(&dst, b"cde");
    }

    #[test]
    fn blit_from_c_copies_to_destination_offset() {
        let mut dst = *b"abcdef";

        blit_from_c(b"XY", &mut dst, 2, 2).unwrap();

        assert_eq!(&dst, b"abXYef");
    }

    #[test]
    fn strlen_stops_at_first_nul() {
        assert_eq!(strlen(b"abc\0def").unwrap(), 3);
    }

    #[test]
    fn null_pointer_helpers_match_c_stub() {
        assert_eq!(null_pointer(), 0);
        assert!(pointer_is_null(0));
        assert!(!pointer_is_null(1));
    }
}
