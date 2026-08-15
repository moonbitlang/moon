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
use windows_sys::Win32::Storage::FileSystem::{FILE_ACTION_MODIFIED, FILE_NOTIFY_INFORMATION};

const EVENT_SIZE: usize = std::mem::size_of::<FILE_NOTIFY_INFORMATION>();

ported_fns! {
    #[ported(
        source = "src/fs/watch_windows.c",
        original = "moonbitlang_async_has_ReadDirectoryChangesExW"
    )]
    pub(crate) fn has_read_directory_changes_ex() -> bool {
        false
    }

    #[ported(
        source = "src/fs/watch_windows.c",
        original = "moonbitlang_async_watcher_event_buffer_size"
    )]
    pub(crate) fn event_buffer_size() -> u32 {
        u32::try_from(EVENT_SIZE * 16384).expect("Windows watcher buffer size fits u32")
    }

    #[ported(
        source = "src/fs/watch_windows.c",
        original = "moonbitlang_async_watcher_event_get_size"
    )]
    pub(crate) fn event_get_size(buffer: &[u8], offset: u32) -> AsyncHostResult<u32> {
        Ok(event_at(buffer, offset)?.NextEntryOffset)
    }

    #[ported(
        source = "src/fs/watch_windows.c",
        original = "moonbitlang_async_watcher_event_is_modify_event"
    )]
    pub(crate) fn event_is_modify(buffer: &[u8], offset: u32) -> AsyncHostResult<bool> {
        Ok(event_at(buffer, offset)?.Action == FILE_ACTION_MODIFIED)
    }

    #[ported(
        source = "src/fs/watch_windows.c",
        original = "moonbitlang_async_watcher_event_get_path_len"
    )]
    pub(crate) fn event_get_path_len(buffer: &[u8], offset: u32) -> AsyncHostResult<u32> {
        Ok(event_at(buffer, offset)?.FileNameLength)
    }

    #[ported(
        source = "src/fs/watch_windows.c",
        original = "moonbitlang_async_watcher_event_get_path_offset"
    )]
    pub(crate) fn event_get_path_offset() -> u32 {
        u32::try_from(std::mem::offset_of!(FILE_NOTIFY_INFORMATION, FileName))
            .expect("Windows watcher path offset fits u32")
    }

    #[ported(
        source = "src/fs/watch_windows.c",
        original = "moonbitlang_async_watcher_event_get_file_id"
    )]
    pub(crate) fn event_get_file_id(_buffer: &[u8], _offset: u32) -> AsyncHostResult<u64> {
        Err(AsyncHostError::Inval)
    }

    #[ported(
        source = "src/fs/watch_windows.c",
        original = "moonbitlang_async_watcher_event_get_parent_file_id"
    )]
    pub(crate) fn event_get_parent_file_id(
        _buffer: &[u8],
        _offset: u32,
    ) -> AsyncHostResult<u64> {
        Err(AsyncHostError::Inval)
    }
}

fn event_at(buffer: &[u8], offset: u32) -> AsyncHostResult<FILE_NOTIFY_INFORMATION> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let end = offset
        .checked_add(EVENT_SIZE)
        .ok_or(AsyncHostError::Fault)?;
    if end > buffer.len() {
        return Err(AsyncHostError::Fault);
    }
    Ok(unsafe { std::ptr::read_unaligned(buffer.as_ptr().add(offset).cast()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_windows_watcher_uses_basic_event_layout() {
        assert!(!has_read_directory_changes_ex());
        assert_eq!(
            event_buffer_size(),
            u32::try_from(std::mem::size_of::<FILE_NOTIFY_INFORMATION>() * 16384).unwrap()
        );
        assert_eq!(
            event_get_path_offset(),
            u32::try_from(std::mem::offset_of!(FILE_NOTIFY_INFORMATION, FileName)).unwrap()
        );
    }

    #[test]
    fn basic_event_accessors_read_unaligned_headers() {
        let offset = 3usize;
        let mut buffer = vec![0; offset + EVENT_SIZE];
        let mut event = unsafe { std::mem::zeroed::<FILE_NOTIFY_INFORMATION>() };
        event.NextEntryOffset = EVENT_SIZE as u32;
        event.Action = FILE_ACTION_MODIFIED;
        event.FileNameLength = 4;
        unsafe {
            std::ptr::write_unaligned(buffer.as_mut_ptr().add(offset).cast(), event);
        }

        assert_eq!(
            event_get_size(&buffer, offset as u32),
            Ok(EVENT_SIZE as u32)
        );
        assert_eq!(event_is_modify(&buffer, offset as u32), Ok(true));
        assert_eq!(event_get_path_len(&buffer, offset as u32), Ok(4));
    }

    #[test]
    fn extended_event_accessors_are_rejected() {
        assert_eq!(event_get_file_id(&[], 0), Err(AsyncHostError::Inval));
        assert_eq!(event_get_parent_file_id(&[], 0), Err(AsyncHostError::Inval));
    }
}
