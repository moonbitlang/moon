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
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::{BOOL, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ACTION_MODIFIED, FILE_NOTIFY_CHANGE, FILE_NOTIFY_EXTENDED_INFORMATION,
    FILE_NOTIFY_INFORMATION, READ_DIRECTORY_NOTIFY_INFORMATION_CLASS,
    ReadDirectoryNotifyExtendedInformation,
};
use windows_sys::Win32::System::IO::{LPOVERLAPPED_COMPLETION_ROUTINE, OVERLAPPED};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

const BASIC_EVENT_SIZE: usize = std::mem::size_of::<FILE_NOTIFY_INFORMATION>();
const EXTENDED_EVENT_SIZE: usize = std::mem::size_of::<FILE_NOTIFY_EXTENDED_INFORMATION>();
// ReadDirectoryChanges{Ex}W rejects buffers larger than 64 KiB for network
// directories. Use the same capacity everywhere so local and remote watches
// have identical submission behavior.
const EVENT_BUFFER_SIZE: usize = 64 * 1024;

type ReadDirectoryChangesExWFn = unsafe extern "system" fn(
    HANDLE,
    *mut core::ffi::c_void,
    u32,
    BOOL,
    FILE_NOTIFY_CHANGE,
    *mut u32,
    *mut OVERLAPPED,
    LPOVERLAPPED_COMPLETION_ROUTINE,
    READ_DIRECTORY_NOTIFY_INFORMATION_CLASS,
) -> BOOL;

pub(crate) unsafe fn read_directory_changes_extended(
    directory: HANDLE,
    buffer: *mut core::ffi::c_void,
    len: u32,
    watch_subtree: BOOL,
    notify_filter: FILE_NOTIFY_CHANGE,
    bytes_returned: *mut u32,
    overlapped: *mut OVERLAPPED,
) -> Option<BOOL> {
    static FUNCTION: OnceLock<Option<ReadDirectoryChangesExWFn>> = OnceLock::new();
    let function = FUNCTION
        .get_or_init(|| unsafe {
            let kernel32 =
                GetModuleHandleW("kernel32.dll\0".encode_utf16().collect::<Vec<_>>().as_ptr());
            if kernel32.is_null() {
                return None;
            }
            GetProcAddress(kernel32, c"ReadDirectoryChangesExW".as_ptr().cast()).map(|function| {
                // GetProcAddress returned the named Kernel32 export. Windows defines
                // this exact system-call signature for ReadDirectoryChangesExW.
                std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    ReadDirectoryChangesExWFn,
                >(function)
            })
        })
        .as_ref()
        .copied()?;
    Some(unsafe {
        function(
            directory,
            buffer,
            len,
            watch_subtree,
            notify_filter,
            bytes_returned,
            overlapped,
            None,
            ReadDirectoryNotifyExtendedInformation,
        )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EventLayout {
    Basic,
    Extended,
}

#[derive(Debug)]
pub(crate) struct EventBuffer {
    // ReadDirectoryChanges{Ex}W requires a DWORD-aligned output buffer. Keep
    // the allocation typed as u32; byte slices are only temporary views over
    // the same initialized storage.
    words: Box<[u32]>,
    layout: Option<EventLayout>,
    completed_len: usize,
}

impl EventBuffer {
    pub(crate) fn new() -> Self {
        let len = usize::try_from(event_buffer_size())
            .expect("Windows watcher buffer size is nonnegative");
        assert!(len.is_multiple_of(std::mem::size_of::<u32>()));
        Self {
            words: vec![0; len / std::mem::size_of::<u32>()].into_boxed_slice(),
            layout: None,
            completed_len: 0,
        }
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: u32 storage is initialized, and this view covers exactly the
        // same allocation without outliving the exclusive borrow.
        unsafe {
            std::slice::from_raw_parts_mut(
                self.words.as_mut_ptr().cast(),
                self.words.len() * std::mem::size_of::<u32>(),
            )
        }
    }

    pub(crate) fn capacity(&self) -> usize {
        self.words.len() * std::mem::size_of::<u32>()
    }

    pub(crate) fn begin_read(&mut self, layout: EventLayout) {
        self.layout = Some(layout);
        self.completed_len = 0;
    }

    pub(crate) fn complete_read(&mut self, len: usize) -> AsyncHostResult<()> {
        if len > self.capacity() {
            return Err(AsyncHostError::Fault);
        }
        self.completed_len = len;
        Ok(())
    }

    fn layout(&self) -> AsyncHostResult<EventLayout> {
        self.layout.ok_or(AsyncHostError::Inval)
    }

    fn completed_bytes(&self) -> &[u8] {
        // SAFETY: u32 storage is initialized, and completed_len was checked
        // against this allocation's byte capacity before it was recorded.
        unsafe { std::slice::from_raw_parts(self.words.as_ptr().cast(), self.completed_len) }
    }
}

pub(crate) fn event_buffer_size() -> u32 {
    u32::try_from(EVENT_BUFFER_SIZE).expect("Windows watcher buffer size fits u32")
}

ported_fns! {
    #[ported(
        source = "src/fs/watch_windows.c",
        original = "moonbitlang_async_watcher_event_get_size"
    )]
    pub(crate) fn event_get_size(buffer: &EventBuffer, offset: u32) -> AsyncHostResult<u32> {
        Ok(match buffer.layout()? {
            EventLayout::Basic => basic_event_at(buffer.completed_bytes(), offset)?.NextEntryOffset,
            EventLayout::Extended => extended_event_at(buffer.completed_bytes(), offset)?.NextEntryOffset,
        })
    }

    #[ported(
        source = "src/fs/watch_windows.c",
        original = "moonbitlang_async_watcher_event_is_modify_event"
    )]
    pub(crate) fn event_is_modify(
        buffer: &EventBuffer,
        offset: u32,
    ) -> AsyncHostResult<bool> {
        let action = match buffer.layout()? {
            EventLayout::Basic => basic_event_at(buffer.completed_bytes(), offset)?.Action,
            EventLayout::Extended => extended_event_at(buffer.completed_bytes(), offset)?.Action,
        };
        Ok(action == FILE_ACTION_MODIFIED)
    }
}

pub(crate) fn event_path_len(buffer: &EventBuffer, offset: u32) -> AsyncHostResult<u32> {
    u32::try_from(event_path_bytes(buffer, offset)?.len() / std::mem::size_of::<u16>())
        .map_err(|_| AsyncHostError::Fault)
}

pub(crate) fn event_path_units(buffer: &EventBuffer, offset: u32) -> AsyncHostResult<Vec<u16>> {
    Ok(event_path_bytes(buffer, offset)?
        .chunks_exact(std::mem::size_of::<u16>())
        .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
        .collect())
}

fn event_path_bytes(buffer: &EventBuffer, offset: u32) -> AsyncHostResult<&[u8]> {
    let (path_offset, path_len) = match buffer.layout()? {
        EventLayout::Basic => {
            let event = basic_event_at(buffer.completed_bytes(), offset)?;
            (
                std::mem::offset_of!(FILE_NOTIFY_INFORMATION, FileName),
                event.FileNameLength,
            )
        }
        EventLayout::Extended => {
            let event = extended_event_at(buffer.completed_bytes(), offset)?;
            (
                std::mem::offset_of!(FILE_NOTIFY_EXTENDED_INFORMATION, FileName),
                event.FileNameLength,
            )
        }
    };
    let event_offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let path_offset = event_offset
        .checked_add(path_offset)
        .ok_or(AsyncHostError::Fault)?;
    let path_len = usize::try_from(path_len).map_err(|_| AsyncHostError::Fault)?;
    let path_end = path_offset
        .checked_add(path_len)
        .ok_or(AsyncHostError::Fault)?;
    let path = buffer
        .completed_bytes()
        .get(path_offset..path_end)
        .ok_or(AsyncHostError::Fault)?;
    if !path.len().is_multiple_of(std::mem::size_of::<u16>()) {
        return Err(AsyncHostError::Inval);
    }
    Ok(path)
}

pub(crate) fn event_has_file_ids(buffer: &EventBuffer) -> AsyncHostResult<bool> {
    Ok(buffer.layout()? == EventLayout::Extended)
}

pub(crate) fn event_dirty_file_id(buffer: &EventBuffer, offset: u32) -> AsyncHostResult<u64> {
    if buffer.layout()? != EventLayout::Extended {
        return Err(AsyncHostError::Inval);
    }
    let event = extended_event_at(buffer.completed_bytes(), offset)?;
    Ok(if event.Action == FILE_ACTION_MODIFIED {
        event.FileId as u64
    } else {
        event.ParentFileId as u64
    })
}

fn basic_event_at(buffer: &[u8], offset: u32) -> AsyncHostResult<FILE_NOTIFY_INFORMATION> {
    event_at(buffer, offset, BASIC_EVENT_SIZE)
}

fn extended_event_at(
    buffer: &[u8],
    offset: u32,
) -> AsyncHostResult<FILE_NOTIFY_EXTENDED_INFORMATION> {
    event_at(buffer, offset, EXTENDED_EVENT_SIZE)
}

fn event_at<T: Copy>(buffer: &[u8], offset: u32, size: usize) -> AsyncHostResult<T> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let end = offset.checked_add(size).ok_or(AsyncHostError::Fault)?;
    if end > buffer.len() {
        return Err(AsyncHostError::Fault);
    }
    Ok(unsafe { std::ptr::read_unaligned(buffer.as_ptr().add(offset).cast()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_event_accessors_preserve_raw_utf16_path() {
        let offset = 3usize;
        let mut buffer = EventBuffer::new();
        buffer.begin_read(EventLayout::Basic);
        let mut event = unsafe { std::mem::zeroed::<FILE_NOTIFY_INFORMATION>() };
        event.NextEntryOffset = BASIC_EVENT_SIZE as u32;
        event.Action = FILE_ACTION_MODIFIED;
        event.FileNameLength = 4;
        unsafe {
            std::ptr::write_unaligned(buffer.as_mut_slice().as_mut_ptr().add(offset).cast(), event);
        }
        let path_offset = offset + std::mem::offset_of!(FILE_NOTIFY_INFORMATION, FileName);
        buffer.as_mut_slice()[path_offset..path_offset + 4].copy_from_slice(&[0x00, 0xd8, b'b', 0]);
        buffer.complete_read(path_offset + 4).unwrap();

        assert_eq!(
            event_get_size(&buffer, offset as u32),
            Ok(BASIC_EVENT_SIZE as u32)
        );
        assert_eq!(event_is_modify(&buffer, offset as u32), Ok(true));
        assert_eq!(event_path_len(&buffer, offset as u32), Ok(2));
        assert_eq!(
            event_path_units(&buffer, offset as u32),
            Ok(vec![0xd800, u16::from(b'b')])
        );
        assert_eq!(event_has_file_ids(&buffer), Ok(false));
        assert_eq!(
            event_dirty_file_id(&buffer, offset as u32),
            Err(AsyncHostError::Inval)
        );
    }

    #[test]
    fn extended_event_accessors_use_the_extended_layout() {
        let offset = 5usize;
        let mut buffer = EventBuffer::new();
        buffer.begin_read(EventLayout::Extended);
        let mut event = unsafe { std::mem::zeroed::<FILE_NOTIFY_EXTENDED_INFORMATION>() };
        event.NextEntryOffset = EXTENDED_EVENT_SIZE as u32;
        event.Action = FILE_ACTION_MODIFIED;
        event.FileId = 42;
        event.ParentFileId = 24;
        event.FileNameLength = 4;
        unsafe {
            std::ptr::write_unaligned(buffer.as_mut_slice().as_mut_ptr().add(offset).cast(), event);
        }
        let path_offset = offset + std::mem::offset_of!(FILE_NOTIFY_EXTENDED_INFORMATION, FileName);
        buffer.as_mut_slice()[path_offset..path_offset + 4].copy_from_slice(&[b'c', 0, b'd', 0]);
        buffer.complete_read(path_offset + 4).unwrap();

        assert_eq!(
            event_get_size(&buffer, offset as u32),
            Ok(EXTENDED_EVENT_SIZE as u32)
        );
        assert_eq!(event_is_modify(&buffer, offset as u32), Ok(true));
        assert_eq!(event_path_len(&buffer, offset as u32), Ok(2));
        assert_eq!(
            event_path_units(&buffer, offset as u32),
            Ok(vec![u16::from(b'c'), u16::from(b'd')])
        );
        assert_eq!(event_has_file_ids(&buffer), Ok(true));
        assert_eq!(event_dirty_file_id(&buffer, offset as u32), Ok(42));
    }

    #[test]
    fn event_accessors_reject_an_uninitialized_layout() {
        let buffer = EventBuffer::new();
        assert_eq!(event_get_size(&buffer, 0), Err(AsyncHostError::Inval));
        assert_eq!(event_path_len(&buffer, 0), Err(AsyncHostError::Inval));
        assert_eq!(event_has_file_ids(&buffer), Err(AsyncHostError::Inval));
    }
}
