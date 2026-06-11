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

pub(crate) const HEADER_LEN: usize = 24;
pub(crate) const BUFFER_MIN_SIZE: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntryRecord {
    pub(crate) name: Vec<u8>,
    pub(crate) is_dir: i32,
    pub(crate) is_hidden: bool,
    pub(crate) file_id: u64,
}

impl EntryRecord {
    pub(crate) fn encoded_len(&self) -> AsyncHostResult<usize> {
        aligned_record_len(self.name.len())
    }

    pub(crate) fn encode_into(&self, dst: &mut Vec<u8>) -> AsyncHostResult<()> {
        let len = self.encoded_len()?;
        let len_u32 = u32::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let name_len = u32::try_from(self.name.len()).map_err(|_| AsyncHostError::Fault)?;

        dst.extend_from_slice(&len_u32.to_le_bytes());
        dst.extend_from_slice(&name_len.to_le_bytes());
        dst.extend_from_slice(&self.is_dir.to_le_bytes());
        dst.extend_from_slice(&(self.is_hidden as u32).to_le_bytes());
        dst.extend_from_slice(&self.file_id.to_le_bytes());
        dst.extend_from_slice(&self.name);
        dst.resize(dst.len() + len - HEADER_LEN - self.name.len(), 0);
        Ok(())
    }
}

fn aligned_record_len(name_len: usize) -> AsyncHostResult<usize> {
    let len = HEADER_LEN
        .checked_add(name_len)
        .ok_or(AsyncHostError::Fault)?;
    Ok((len + 7) & !7)
}

fn read_u32(buf: &[u8], offset: i32, field_offset: usize) -> AsyncHostResult<u32> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let start = offset
        .checked_add(field_offset)
        .ok_or(AsyncHostError::Fault)?;
    let end = start.checked_add(4).ok_or(AsyncHostError::Fault)?;
    let bytes = buf.get(start..end).ok_or(AsyncHostError::Fault)?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn read_i32(buf: &[u8], offset: i32, field_offset: usize) -> AsyncHostResult<i32> {
    Ok(read_u32(buf, offset, field_offset)? as i32)
}

fn read_u64(buf: &[u8], offset: i32, field_offset: usize) -> AsyncHostResult<u64> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let start = offset
        .checked_add(field_offset)
        .ok_or(AsyncHostError::Fault)?;
    let end = start.checked_add(8).ok_or(AsyncHostError::Fault)?;
    let bytes = buf.get(start..end).ok_or(AsyncHostError::Fault)?;
    Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
}

ported_fns! {
    #[ported(
        source = "src/fs/dir.c",
        original = "moonbitlang_async_dir_buffer_min_size"
    )]
    pub(crate) fn buffer_min_size() -> i32 {
        BUFFER_MIN_SIZE as i32
    }

    #[ported(
        source = "src/fs/dir.c",
        original = "moonbitlang_async_dir_entry_length"
    )]
    pub(crate) fn entry_length(buf: &[u8], offset: i32) -> AsyncHostResult<i32> {
        i32::try_from(read_u32(buf, offset, 0)?).map_err(|_| AsyncHostError::Fault)
    }

    #[ported(
        source = "src/fs/dir.c",
        original = "moonbitlang_async_dir_entry_get_name_len"
    )]
    pub(crate) fn entry_name_len(buf: &[u8], offset: i32) -> AsyncHostResult<i32> {
        i32::try_from(read_u32(buf, offset, 4)?).map_err(|_| AsyncHostError::Fault)
    }

    #[ported(
        source = "src/fs/dir.c",
        original = "moonbitlang_async_dir_entry_get_name"
    )]
    pub(crate) fn entry_name_ptr(buf_ptr: i32, offset: i32) -> AsyncHostResult<i32> {
        buf_ptr
            .checked_add(offset)
            .and_then(|ptr| ptr.checked_add(HEADER_LEN as i32))
            .ok_or(AsyncHostError::Fault)
    }

    #[ported(
        source = "src/fs/dir.c",
        original = "moonbitlang_async_dir_entry_is_dir"
    )]
    pub(crate) fn entry_is_dir(buf: &[u8], offset: i32) -> AsyncHostResult<i32> {
        read_i32(buf, offset, 8)
    }

    #[ported(
        source = "src/fs/dir.c",
        original = "moonbitlang_async_dir_entry_is_hidden"
    )]
    pub(crate) fn entry_is_hidden(buf: &[u8], offset: i32) -> AsyncHostResult<bool> {
        Ok(read_u32(buf, offset, 12)? != 0)
    }

    #[ported(
        source = "src/fs/dir.c",
        original = "moonbitlang_async_dir_entry_get_file_id"
    )]
    pub(crate) fn entry_file_id(buf: &[u8], offset: i32) -> AsyncHostResult<u64> {
        read_u64(buf, offset, 16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_use_fixed_little_endian_layout() {
        let record = EntryRecord {
            name: b"abc".to_vec(),
            is_dir: 1,
            is_hidden: false,
            file_id: 0x0102_0304_0506_0708,
        };
        let mut buf = Vec::new();

        record.encode_into(&mut buf).unwrap();

        assert_eq!(entry_length(&buf, 0), Ok(32));
        assert_eq!(entry_name_len(&buf, 0), Ok(3));
        assert_eq!(entry_is_dir(&buf, 0), Ok(1));
        assert_eq!(entry_is_hidden(&buf, 0), Ok(false));
        assert_eq!(entry_file_id(&buf, 0), Ok(0x0102_0304_0506_0708));
        assert_eq!(&buf[HEADER_LEN..HEADER_LEN + 3], b"abc");
    }
}
