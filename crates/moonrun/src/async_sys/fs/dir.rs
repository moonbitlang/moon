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

#[cfg(any(target_os = "linux", target_os = "macos"))]
const NAME_MAX: usize = 255;

#[cfg(target_os = "linux")]
const LINUX_DIRENT_SIZE: usize = 24;
#[cfg(target_os = "linux")]
const LINUX_D_INO_OFFSET: usize = 0;
#[cfg(target_os = "linux")]
const LINUX_D_RECLEN_OFFSET: usize = 16;
#[cfg(target_os = "linux")]
const LINUX_D_TYPE_OFFSET: usize = 18;
#[cfg(target_os = "linux")]
const LINUX_D_NAME_OFFSET: usize = 19;

#[cfg(target_os = "macos")]
const MAC_DIRENT_SIZE: usize = 44;
#[cfg(target_os = "macos")]
const MAC_D_RECLEN_OFFSET: usize = 0;
#[cfg(target_os = "macos")]
const MAC_D_ATTRS_OFFSET: usize = 4;
#[cfg(target_os = "macos")]
const MAC_D_NAME_OFFSET: usize = 24;
#[cfg(target_os = "macos")]
const MAC_D_NAME_LENGTH_OFFSET: usize = 28;
#[cfg(target_os = "macos")]
const MAC_D_TYPE_OFFSET: usize = 32;
#[cfg(target_os = "macos")]
const MAC_D_FILEID_OFFSET: usize = 36;

ported_fns! {
    #[ported(
        source = "src/fs/dir.c",
        original = "moonbitlang_async_dir_buffer_min_size"
    )]
    pub(crate) fn buffer_min_size() -> i32 {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::MAX_PATH;
            use windows_sys::Win32::Storage::FileSystem::FILE_ID_BOTH_DIR_INFO;

            (std::mem::size_of::<FILE_ID_BOTH_DIR_INFO>() + MAX_PATH as usize) as i32
        }

        #[cfg(target_os = "macos")]
        {
            (MAC_DIRENT_SIZE + NAME_MAX) as i32
        }

        #[cfg(target_os = "linux")]
        {
            (LINUX_DIRENT_SIZE + NAME_MAX) as i32
        }
    }

    #[ported(
        source = "src/fs/dir.c",
        original = "moonbitlang_async_dir_entry_length"
    )]
    pub(crate) fn entry_length(buf: &[u8], buf_ptr: i32, offset: i32) -> AsyncHostResult<i32> {
        #[cfg(windows)]
        {
            let entry = windows_entry(buf, buf_ptr, offset)?;
            i32::try_from(entry.NextEntryOffset).map_err(|_| AsyncHostError::Fault)
        }

        #[cfg(target_os = "macos")]
        {
            let entry = entry_offset(buf_ptr, offset)?;
            i32::try_from(read_u32_ne(buf, entry + MAC_D_RECLEN_OFFSET)?)
                .map_err(|_| AsyncHostError::Fault)
        }

        #[cfg(target_os = "linux")]
        {
            let entry = entry_offset(buf_ptr, offset)?;
            Ok(i32::from(read_u16_ne(buf, entry + LINUX_D_RECLEN_OFFSET)?))
        }
    }

    #[ported(
        source = "src/fs/dir.c",
        original = "moonbitlang_async_dir_entry_get_name_len"
    )]
    pub(crate) fn entry_name_len(buf: &[u8], buf_ptr: i32, offset: i32) -> AsyncHostResult<i32> {
        #[cfg(windows)]
        {
            let entry = windows_entry(buf, buf_ptr, offset)?;
            i32::try_from(entry.FileNameLength).map_err(|_| AsyncHostError::Fault)
        }

        #[cfg(target_os = "macos")]
        {
            let (_, len) = mac_name_range(buf, buf_ptr, offset)?;
            i32::try_from(len).map_err(|_| AsyncHostError::Fault)
        }

        #[cfg(target_os = "linux")]
        {
            let (_, len) = linux_name_range(buf, buf_ptr, offset)?;
            i32::try_from(len).map_err(|_| AsyncHostError::Fault)
        }
    }

    #[ported(
        source = "src/fs/dir.c",
        original = "moonbitlang_async_dir_entry_get_name_offset"
    )]
    pub(crate) fn entry_name_offset(
        buf: &[u8],
        buf_ptr: i32,
        offset: i32,
    ) -> AsyncHostResult<i32> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Storage::FileSystem::FILE_ID_BOTH_DIR_INFO;

            windows_name_range(buf, buf_ptr, offset)?;
            i32::try_from(std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName))
                .map_err(|_| AsyncHostError::Fault)
        }

        #[cfg(target_os = "macos")]
        {
            let name_offset = mac_name_offset(buf, buf_ptr, offset)?;
            i32::try_from(
                MAC_D_NAME_OFFSET
                    .checked_add(name_offset)
                    .ok_or(AsyncHostError::Fault)?,
            )
            .map_err(|_| AsyncHostError::Fault)
        }

        #[cfg(target_os = "linux")]
        {
            linux_name_range(buf, buf_ptr, offset)?;
            i32::try_from(LINUX_D_NAME_OFFSET).map_err(|_| AsyncHostError::Fault)
        }
    }

    #[ported(
        source = "src/fs/dir.c",
        original = "moonbitlang_async_dir_entry_is_dir"
    )]
    pub(crate) fn entry_is_dir(buf: &[u8], buf_ptr: i32, offset: i32) -> AsyncHostResult<i32> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
            };

            let entry = windows_entry(buf, buf_ptr, offset)?;
            Ok(((entry.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) == 0
                && (entry.FileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0) as i32)
        }

        #[cfg(target_os = "macos")]
        {
            let entry = entry_offset(buf_ptr, offset)?;
            let attrs = read_u32_ne(buf, entry + MAC_D_ATTRS_OFFSET)?;
            if (attrs & libc::ATTR_CMN_OBJTYPE) == 0 {
                return Ok(-1);
            }

            match read_i32_ne(buf, entry + MAC_D_TYPE_OFFSET)? {
                // vnode.h defines VNON = 0 and VDIR = 2. libc does not expose
                // these macOS constants on every supported Rust target.
                0 => Ok(-1),
                2 => Ok(1),
                _ => Ok(0),
            }
        }

        #[cfg(target_os = "linux")]
        {
            let entry = entry_offset(buf_ptr, offset)?;
            match read_u8(buf, entry + LINUX_D_TYPE_OFFSET)? {
                libc::DT_UNKNOWN => Ok(-1),
                libc::DT_DIR => Ok(1),
                _ => Ok(0),
            }
        }
    }

    #[ported(
        source = "src/fs/dir.c",
        original = "moonbitlang_async_dir_entry_is_hidden"
    )]
    pub(crate) fn entry_is_hidden(buf: &[u8], buf_ptr: i32, offset: i32) -> AsyncHostResult<bool> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_HIDDEN;

            let entry = windows_entry(buf, buf_ptr, offset)?;
            Ok((entry.FileAttributes & FILE_ATTRIBUTE_HIDDEN) != 0)
        }

        #[cfg(target_os = "macos")]
        {
            let (start, _) = mac_name_range(buf, buf_ptr, offset)?;
            Ok(read_u8(buf, start)? == b'.')
        }

        #[cfg(target_os = "linux")]
        {
            let (start, _) = linux_name_range(buf, buf_ptr, offset)?;
            Ok(read_u8(buf, start)? == b'.')
        }
    }

    #[ported(
        source = "src/fs/dir.c",
        original = "moonbitlang_async_dir_entry_get_file_id"
    )]
    pub(crate) fn entry_file_id(buf: &[u8], buf_ptr: i32, offset: i32) -> AsyncHostResult<u64> {
        #[cfg(windows)]
        {
            let entry = windows_entry(buf, buf_ptr, offset)?;
            Ok(entry.FileId as u64)
        }

        #[cfg(target_os = "macos")]
        {
            let entry = entry_offset(buf_ptr, offset)?;
            read_u64_ne(buf, entry + MAC_D_FILEID_OFFSET)
        }

        #[cfg(target_os = "linux")]
        {
            let entry = entry_offset(buf_ptr, offset)?;
            read_u64_ne(buf, entry + LINUX_D_INO_OFFSET)
        }
    }
}

fn entry_offset(buf_ptr: i32, offset: i32) -> AsyncHostResult<usize> {
    let ptr = buf_ptr.checked_add(offset).ok_or(AsyncHostError::Fault)?;
    usize::try_from(ptr).map_err(|_| AsyncHostError::Fault)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn read_u8(buf: &[u8], offset: usize) -> AsyncHostResult<u8> {
    buf.get(offset).copied().ok_or(AsyncHostError::Fault)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn read_array<const N: usize>(buf: &[u8], offset: usize) -> AsyncHostResult<[u8; N]> {
    let end = offset.checked_add(N).ok_or(AsyncHostError::Fault)?;
    buf.get(offset..end)
        .ok_or(AsyncHostError::Fault)?
        .try_into()
        .map_err(|_| AsyncHostError::Fault)
}

#[cfg(target_os = "linux")]
fn read_u16_ne(buf: &[u8], offset: usize) -> AsyncHostResult<u16> {
    Ok(u16::from_ne_bytes(read_array(buf, offset)?))
}

#[cfg(target_os = "macos")]
fn read_u32_ne(buf: &[u8], offset: usize) -> AsyncHostResult<u32> {
    Ok(u32::from_ne_bytes(read_array(buf, offset)?))
}

#[cfg(target_os = "macos")]
fn read_i32_ne(buf: &[u8], offset: usize) -> AsyncHostResult<i32> {
    Ok(i32::from_ne_bytes(read_array(buf, offset)?))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn read_u64_ne(buf: &[u8], offset: usize) -> AsyncHostResult<u64> {
    Ok(u64::from_ne_bytes(read_array(buf, offset)?))
}

#[cfg(target_os = "linux")]
fn linux_name_range(buf: &[u8], buf_ptr: i32, offset: i32) -> AsyncHostResult<(usize, usize)> {
    let entry = entry_offset(buf_ptr, offset)?;
    let reclen = usize::from(read_u16_ne(buf, entry + LINUX_D_RECLEN_OFFSET)?);
    let name_start = entry
        .checked_add(LINUX_D_NAME_OFFSET)
        .ok_or(AsyncHostError::Fault)?;
    let record_end = entry.checked_add(reclen).ok_or(AsyncHostError::Fault)?;
    let name = buf
        .get(name_start..record_end)
        .ok_or(AsyncHostError::Fault)?;
    let name_len = name
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(AsyncHostError::Fault)?;
    Ok((name_start, name_len))
}

#[cfg(target_os = "macos")]
fn mac_name_range(buf: &[u8], buf_ptr: i32, offset: i32) -> AsyncHostResult<(usize, usize)> {
    let entry = entry_offset(buf_ptr, offset)?;
    let reclen = usize::try_from(read_u32_ne(buf, entry + MAC_D_RECLEN_OFFSET)?)
        .map_err(|_| AsyncHostError::Fault)?;
    let name_offset = mac_name_offset(buf, buf_ptr, offset)?;
    let name_len = usize::try_from(read_u32_ne(buf, entry + MAC_D_NAME_LENGTH_OFFSET)?)
        .map_err(|_| AsyncHostError::Fault)?;
    let name_len = name_len.checked_sub(1).ok_or(AsyncHostError::Fault)?;
    let name_start = entry
        .checked_add(MAC_D_NAME_OFFSET)
        .and_then(|offset| offset.checked_add(name_offset))
        .ok_or(AsyncHostError::Fault)?;
    let name_end = name_start
        .checked_add(name_len)
        .ok_or(AsyncHostError::Fault)?;
    let record_end = entry.checked_add(reclen).ok_or(AsyncHostError::Fault)?;
    if name_end > record_end {
        return Err(AsyncHostError::Fault);
    }
    Ok((name_start, name_len))
}

#[cfg(target_os = "macos")]
fn mac_name_offset(buf: &[u8], buf_ptr: i32, offset: i32) -> AsyncHostResult<usize> {
    let entry = entry_offset(buf_ptr, offset)?;
    usize::try_from(read_i32_ne(buf, entry + MAC_D_NAME_OFFSET)?).map_err(|_| AsyncHostError::Fault)
}

#[cfg(windows)]
fn windows_entry(
    buf: &[u8],
    buf_ptr: i32,
    offset: i32,
) -> AsyncHostResult<windows_sys::Win32::Storage::FileSystem::FILE_ID_BOTH_DIR_INFO> {
    use windows_sys::Win32::Storage::FileSystem::FILE_ID_BOTH_DIR_INFO;

    let entry = entry_offset(buf_ptr, offset)?;
    let end = entry
        .checked_add(std::mem::size_of::<FILE_ID_BOTH_DIR_INFO>())
        .ok_or(AsyncHostError::Fault)?;
    if end > buf.len() {
        return Err(AsyncHostError::Fault);
    }
    Ok(unsafe { std::ptr::read_unaligned(buf.as_ptr().add(entry).cast()) })
}

#[cfg(windows)]
fn windows_name_range(buf: &[u8], buf_ptr: i32, offset: i32) -> AsyncHostResult<(usize, usize)> {
    use windows_sys::Win32::Storage::FileSystem::FILE_ID_BOTH_DIR_INFO;

    let entry = windows_entry(buf, buf_ptr, offset)?;
    let entry_start = entry_offset(buf_ptr, offset)?;
    let name_start = entry_start
        .checked_add(std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName))
        .ok_or(AsyncHostError::Fault)?;
    let name_len = usize::try_from(entry.FileNameLength).map_err(|_| AsyncHostError::Fault)?;
    let name_end = name_start
        .checked_add(name_len)
        .ok_or(AsyncHostError::Fault)?;
    if name_end > buf.len() {
        return Err(AsyncHostError::Fault);
    }
    Ok((name_start, name_len))
}

#[cfg(test)]
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_entry_accessors_read_native_getdents64_layout() {
        let base = 100usize;
        let mut buf = vec![0; 200];
        buf[base..base + 8].copy_from_slice(&0x0102_0304_0506_0708_u64.to_ne_bytes());
        buf[base + LINUX_D_RECLEN_OFFSET..base + LINUX_D_RECLEN_OFFSET + 2]
            .copy_from_slice(&24_u16.to_ne_bytes());
        buf[base + LINUX_D_TYPE_OFFSET] = libc::DT_DIR;
        buf[base + LINUX_D_NAME_OFFSET..base + LINUX_D_NAME_OFFSET + 4].copy_from_slice(b"abc\0");

        assert_eq!(entry_length(&buf, 100, 0), Ok(24));
        assert_eq!(entry_name_len(&buf, 100, 0), Ok(3));
        assert_eq!(entry_name_offset(&buf, 100, 0), Ok(19));
        assert_eq!(entry_is_dir(&buf, 100, 0), Ok(1));
        assert_eq!(entry_is_hidden(&buf, 100, 0), Ok(false));
        assert_eq!(entry_file_id(&buf, 100, 0), Ok(0x0102_0304_0506_0708));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_entry_accessors_read_native_getattrlistbulk_layout() {
        let base = 100usize;
        let mut buf = vec![0; 200];
        buf[base + MAC_D_RECLEN_OFFSET..base + MAC_D_RECLEN_OFFSET + 4]
            .copy_from_slice(&52_u32.to_ne_bytes());
        buf[base + MAC_D_ATTRS_OFFSET..base + MAC_D_ATTRS_OFFSET + 4]
            .copy_from_slice(&libc::ATTR_CMN_OBJTYPE.to_ne_bytes());
        buf[base + MAC_D_NAME_OFFSET..base + MAC_D_NAME_OFFSET + 4]
            .copy_from_slice(&20_i32.to_ne_bytes());
        buf[base + MAC_D_NAME_LENGTH_OFFSET..base + MAC_D_NAME_LENGTH_OFFSET + 4]
            .copy_from_slice(&4_u32.to_ne_bytes());
        buf[base + MAC_D_TYPE_OFFSET..base + MAC_D_TYPE_OFFSET + 4]
            .copy_from_slice(&2_i32.to_ne_bytes());
        buf[base + MAC_D_FILEID_OFFSET..base + MAC_D_FILEID_OFFSET + 8]
            .copy_from_slice(&0x0102_0304_0506_0708_u64.to_ne_bytes());
        buf[base + 44..base + 48].copy_from_slice(b"abc\0");

        assert_eq!(entry_length(&buf, 100, 0), Ok(52));
        assert_eq!(entry_name_len(&buf, 100, 0), Ok(3));
        assert_eq!(entry_name_offset(&buf, 100, 0), Ok(44));
        assert_eq!(entry_is_dir(&buf, 100, 0), Ok(1));
        assert_eq!(entry_is_hidden(&buf, 100, 0), Ok(false));
        assert_eq!(entry_file_id(&buf, 100, 0), Ok(0x0102_0304_0506_0708));
    }
}
