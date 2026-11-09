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

use std::os::windows::io::AsRawHandle;

use windows_sys::Win32::Foundation::{ERROR_OPERATION_ABORTED, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Console::{INPUT_RECORD, KEY_EVENT, WINDOW_BUFFER_SIZE_EVENT};
use windows_sys::Win32::System::Threading::{INFINITE, WaitForMultipleObjects};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_MENU;
use windows_sys::core::BOOL;

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::event_loop::thread_pool::with_cancellable_region;
use crate::resource::Resource;

// <https://learn.microsoft.com/windows/console/readconsoleinputex>: exported by
// kernel32 but absent from the SDK headers and import libraries, hence from
// windows-sys. `windows_link` binds it without an import library.
windows_link::link!("kernel32.dll" "system" fn ReadConsoleInputExW(
    input: HANDLE,
    buffer: *mut INPUT_RECORD,
    length: u32,
    read: *mut u32,
    flags: u16,
) -> BOOL);

/// Return immediately, even if the input buffer has no events.
const CONSOLE_READ_NOWAIT: u16 = 0x0002;

/// `TTY_READ_RESIZED` in the native runtime: the wait ended because the
/// window was resized, and no input was consumed.
const TTY_READ_RESIZED: i64 = -1;

fn last_error() -> AsyncHostError {
    AsyncHostError::Native(std::io::Error::last_os_error().raw_os_error().unwrap_or(1))
}

/// Each record produces at most one UTF-16 code unit.
fn records_to_text(records: &[INPUT_RECORD], text: &mut Vec<u16>) -> bool {
    let mut resized = false;
    for record in records {
        match u32::from(record.EventType) {
            KEY_EVENT => {
                // EventType selects the active union member.
                let key = unsafe { record.Event.KeyEvent };
                let unit = unsafe { key.uChar.UnicodeChar };
                // conhost synthesizes the records of VT input with scan code 0,
                // including NUL from Ctrl+Space, Ctrl+2 and Ctrl+@.
                let nul = key.wVirtualScanCode == 0;
                // Alt+Numpad and non-BMP characters can arrive on Alt key-up.
                if (unit != 0 || nul) && (key.bKeyDown != 0 || key.wVirtualKeyCode == VK_MENU) {
                    text.push(unit);
                }
            }
            WINDOW_BUFFER_SIZE_EVENT => resized = true,
            _ => {}
        }
    }
    resized
}

pub(in crate::filesystem::job) fn read_console_input(
    input: &Resource,
    cancel: &Resource,
    len: u32,
    result: &mut Option<Vec<u16>>,
    resized: &mut bool,
) -> AsyncHostResult<i64> {
    const MAX_RECORDS: usize = 256;
    let max_records = len.min(MAX_RECORDS as u32);
    // Reading no records would spin on the signaled console. Reject it before
    // waiting.
    if max_records == 0 {
        return Err(AsyncHostError::Inval);
    }
    let input = input.as_handle()?.as_raw_handle();
    let cancel = cancel.as_handle()?.as_raw_handle();
    let mut records = unsafe { std::mem::zeroed::<[INPUT_RECORD; MAX_RECORDS]>() };
    let mut text = Vec::with_capacity(max_records as usize);
    loop {
        with_cancellable_region(|| {
            let handles = [input, cancel];
            match unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) } {
                WAIT_OBJECT_0 => Ok(()),
                ret if ret == WAIT_OBJECT_0 + 1 => {
                    Err(AsyncHostError::Native(ERROR_OPERATION_ABORTED as i32))
                }
                _ => Err(last_error()),
            }
        })?;
        // Another reader of the console can drain it after the wakeup. A
        // blocking read would then wait for more input, out of reach of the
        // cancellation event, so read without waiting and wait again if the
        // console is empty.
        let mut count = 0;
        if unsafe {
            ReadConsoleInputExW(
                input,
                records.as_mut_ptr(),
                max_records,
                &mut count,
                CONSOLE_READ_NOWAIT,
            )
        } == 0
        {
            return Err(last_error());
        }
        *resized = records_to_text(&records[..count as usize], &mut text);
        if !text.is_empty() || *resized {
            let ret = if text.is_empty() {
                TTY_READ_RESIZED
            } else {
                text.len() as i64
            };
            *result = Some(text);
            return Ok(ret);
        }
    }
}
