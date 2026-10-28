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

use windows_sys::Win32::Foundation::{ERROR_OPERATION_ABORTED, WAIT_OBJECT_0};
use windows_sys::Win32::System::Console::{
    FOCUS_EVENT, GetNumberOfConsoleInputEvents, INPUT_RECORD, KEY_EVENT, ReadConsoleInputW,
    WINDOW_BUFFER_SIZE_EVENT,
};
use windows_sys::Win32::System::Threading::{INFINITE, ResetEvent, WaitForMultipleObjects};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_MENU;

use super::TTY_READ_RESIZED;
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::event_loop::thread_pool::with_cancellable_region;
use crate::resource::Resource;

fn last_error() -> AsyncHostError {
    AsyncHostError::Native(std::io::Error::last_os_error().raw_os_error().unwrap_or(1))
}

/// Each record produces at most three UTF-16 code units (a focus sequence).
fn records_to_text(records: &[INPUT_RECORD], text: &mut Vec<u16>) -> bool {
    let mut resized = false;
    for record in records {
        match u32::from(record.EventType) {
            KEY_EVENT => {
                // EventType selects the active union member.
                let key = unsafe { record.Event.KeyEvent };
                let unit = unsafe { key.uChar.UnicodeChar };
                // Alt+Numpad and non-BMP characters can arrive on Alt key-up.
                if unit != 0 && (key.bKeyDown != 0 || key.wVirtualKeyCode == VK_MENU) {
                    text.push(unit);
                }
            }
            FOCUS_EVENT => {
                let focused = unsafe { record.Event.FocusEvent.bSetFocus } != 0;
                text.extend_from_slice(&[
                    0x1b,
                    b'[' as u16,
                    if focused { b'I' } else { b'O' } as u16,
                ]);
            }
            WINDOW_BUFFER_SIZE_EVENT => resized = true,
            _ => {}
        }
    }
    resized
}

pub(in crate::filesystem::job) fn read_console_input(
    input: &Resource,
    wakeup: &Resource,
    len: u32,
    result: &mut Option<Vec<u16>>,
    resized: &mut bool,
) -> AsyncHostResult<i64> {
    const MAX_RECORDS: usize = 256;
    let max_records = (len / 3).min(MAX_RECORDS as u32);
    // A shorter buffer cannot hold a focus event. Reject it before consuming input.
    if max_records == 0 {
        return Err(AsyncHostError::Inval);
    }
    let input = input.as_handle()?.as_raw_handle();
    let wakeup = wakeup.as_handle()?.as_raw_handle();
    let mut records = unsafe { std::mem::zeroed::<[INPUT_RECORD; MAX_RECORDS]>() };
    let mut text = Vec::with_capacity(max_records as usize * 3);
    // Discard a previous Job's wake. The cancellation check below also covers
    // cancellation of this Job before or during this reset.
    if unsafe { ResetEvent(wakeup) } == 0 {
        return Err(last_error());
    }
    loop {
        with_cancellable_region(|| {
            let handles = [input, wakeup];
            match unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) } {
                WAIT_OBJECT_0 => Ok(()),
                ret if ret == WAIT_OBJECT_0 + 1 => {
                    Err(AsyncHostError::Native(ERROR_OPERATION_ABORTED as i32))
                }
                _ => Err(last_error()),
            }
        })?;
        let mut available = 0;
        if unsafe { GetNumberOfConsoleInputEvents(input, &mut available) } == 0 {
            return Err(last_error());
        }
        if available == 0 {
            continue;
        }
        let count = with_cancellable_region(|| {
            let mut count = 0;
            if unsafe {
                ReadConsoleInputW(
                    input,
                    records.as_mut_ptr(),
                    available.min(max_records),
                    &mut count,
                )
            } == 0
            {
                return Err(last_error());
            }
            Ok(count)
        })?;
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
