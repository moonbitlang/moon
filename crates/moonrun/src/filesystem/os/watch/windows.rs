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

//! Windows directory change submission and native event-layout selection.

use super::RawFile;
use std::{io, sync::OnceLock};
use windows_sys::Win32::Foundation::{ERROR_INVALID_FUNCTION, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_NOTIFY_CHANGE, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
    READ_DIRECTORY_NOTIFY_INFORMATION_CLASS, ReadDirectoryChangesW,
    ReadDirectoryNotifyExtendedInformation,
};
use windows_sys::Win32::System::IO::{LPOVERLAPPED_COMPLETION_ROUTINE, OVERLAPPED};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::core::BOOL;

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

unsafe fn read_directory_changes_extended(
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
pub(in crate::filesystem) enum EventLayout {
    Basic,
    Extended,
}

/// Submit a recursive directory watch, preferring extended event records.
///
/// # Safety
/// The caller must retain the directory, the DWORD-aligned buffer of `len`
/// bytes, and `overlapped` until the operation completes. Neither allocation
/// may be accessed or moved while pending. `overlapped` must be initialized for
/// this submission and must not be in use by another pending operation.
pub(in crate::filesystem) unsafe fn submit(
    directory: RawFile,
    buffer: *mut u8,
    len: u32,
    overlapped: *mut OVERLAPPED,
) -> io::Result<EventLayout> {
    let notify_filter = FILE_NOTIFY_CHANGE_SIZE
        | FILE_NOTIFY_CHANGE_LAST_WRITE
        | FILE_NOTIFY_CHANGE_FILE_NAME
        | FILE_NOTIFY_CHANGE_DIR_NAME;
    let mut bytes_returned = 0;
    let extended = unsafe {
        read_directory_changes_extended(
            directory,
            buffer.cast(),
            len,
            1,
            notify_filter,
            &mut bytes_returned,
            overlapped,
        )
    };
    match extended {
        Some(success) if success != 0 => return Ok(EventLayout::Extended),
        Some(_) => {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_INVALID_FUNCTION as i32) {
                return Err(error);
            }
            // The extended request was rejected, so no operation owns the
            // OVERLAPPED. Reset it before retrying with the basic layout.
            unsafe {
                let event = (*overlapped).hEvent;
                overlapped.write(std::mem::zeroed());
                (*overlapped).hEvent = event;
            };
            bytes_returned = 0;
        }
        None => {}
    }
    if unsafe {
        ReadDirectoryChangesW(
            directory,
            buffer.cast(),
            len,
            1,
            notify_filter,
            &mut bytes_returned,
            overlapped,
            None,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(EventLayout::Basic)
    }
}
