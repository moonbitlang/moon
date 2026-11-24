// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use super::os::watch::{self, EventLayout};
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::ported_fns;
use crate::resource::Resource;
use std::os::windows::io::AsRawHandle;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ACTION_MODIFIED, FILE_NOTIFY_EXTENDED_INFORMATION, FILE_NOTIFY_INFORMATION,
};
use windows_sys::Win32::System::IO::OVERLAPPED;

const BASIC_EVENT_SIZE: usize = std::mem::size_of::<FILE_NOTIFY_INFORMATION>();
const EXTENDED_EVENT_SIZE: usize = std::mem::size_of::<FILE_NOTIFY_EXTENDED_INFORMATION>();
// ReadDirectoryChanges{Ex}W rejects buffers larger than 64 KiB for network
// directories. Use the same capacity everywhere so local and remote watches
// have identical submission behavior.
const EVENT_BUFFER_SIZE: usize = 64 * 1024;

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

    /// Submit using this buffer's allocation and record the selected layout.
    ///
    /// # Safety
    /// The caller must retain the authorized directory, this allocation, and
    /// `overlapped` until completion, without accessing either buffer while
    /// pending. `overlapped` must be initialized and have no pending operation.
    pub(crate) unsafe fn submit(
        &mut self,
        directory: &Resource,
        overlapped: *mut OVERLAPPED,
    ) -> AsyncHostResult<()> {
        let len = u32::try_from(self.capacity()).map_err(|_| AsyncHostError::Fault)?;
        let layout = unsafe {
            watch::submit(
                directory.as_file()?.as_raw_handle(),
                self.as_mut_slice().as_mut_ptr(),
                len,
                overlapped,
            )?
        };
        self.begin_read(layout);
        Ok(())
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
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

    fn begin_read(&mut self, layout: EventLayout) {
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
    fn submitted_directory_watch_decodes_the_completed_native_layout() {
        use std::os::windows::{
            fs::OpenOptionsExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        };
        use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult};
        use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

        let directory = tempfile::tempdir().unwrap();
        let file = std::fs::OpenOptions::new()
            .access_mode(FILE_LIST_DIRECTORY)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED)
            .open(directory.path())
            .unwrap();
        let resource = Resource::from_file(file, crate::filesystem::FileGrant::UNTRACKED);
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        assert!(!event.is_null());
        let event = unsafe { OwnedHandle::from_raw_handle(event) };
        let mut overlapped: Box<OVERLAPPED> = Box::new(unsafe { std::mem::zeroed() });
        overlapped.hEvent = event.as_raw_handle();
        let mut buffer = EventBuffer::new();
        unsafe { buffer.submit(&resource, overlapped.as_mut()).unwrap() };

        let write = std::fs::write(directory.path().join("changed"), b"contents");
        let wait = unsafe { WaitForSingleObject(event.as_raw_handle(), 5000) };
        // Drain or cancel before asserting: even a failed write or timeout must
        // not free storage while Windows can still write to it.
        let handle = resource.as_file().unwrap().as_raw_handle();
        unsafe { CancelIoEx(handle, overlapped.as_ref()) };
        let mut count = 0;
        let completed = unsafe { GetOverlappedResult(handle, overlapped.as_ref(), &mut count, 1) };
        let error = std::io::Error::last_os_error();
        write.unwrap();
        assert_eq!(wait, WAIT_OBJECT_0);
        assert_ne!(completed, 0, "{error}");
        buffer.complete_read(count as usize).unwrap();
        assert_eq!(
            event_path_units(&buffer, 0).unwrap(),
            "changed".encode_utf16().collect::<Vec<_>>()
        );
        assert!(event_has_file_ids(&buffer).is_ok());
    }

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
