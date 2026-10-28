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

use std::os::windows::io::{
    AsRawHandle, BorrowedHandle, FromRawHandle, IntoRawHandle, OwnedHandle,
};

use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::*;
use windows_sys::Win32::System::Threading::CreateEventW;

use super::TtyResources;
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::resource::Resource;

fn io_error(error: std::io::Error) -> AsyncHostError {
    AsyncHostError::Native(error.raw_os_error().unwrap_or(1))
}

fn last_error() -> AsyncHostError {
    io_error(std::io::Error::last_os_error())
}

fn open_console(name: *const u16) -> AsyncHostResult<OwnedHandle> {
    let handle = unsafe {
        CreateFileW(
            name,
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(last_error());
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

pub(super) fn open() -> AsyncHostResult<TtyResources> {
    let input = open_console(windows_sys::w!("CONIN$"))?;
    let control = open_console(windows_sys::w!("CONOUT$"))?;
    let output = control.try_clone().map_err(io_error)?;
    let wakeup = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
    if wakeup.is_null() {
        return Err(last_error());
    }
    Ok(TtyResources {
        input: Resource::new(input.into_raw_handle()),
        wakeup: Resource::new(wakeup),
        control: Resource::new(control.into_raw_handle()),
        output: Resource::new(output.into_raw_handle()),
    })
}

fn get_mode(handle: BorrowedHandle<'_>) -> AsyncHostResult<u32> {
    let mut mode = 0;
    if unsafe { GetConsoleMode(handle.as_raw_handle(), &mut mode) } == 0 {
        return Err(last_error());
    }
    Ok(mode)
}

fn set_mode(handle: BorrowedHandle<'_>, mode: u32) -> AsyncHostResult<()> {
    if unsafe { SetConsoleMode(handle.as_raw_handle(), mode) } == 0 {
        return Err(last_error());
    }
    Ok(())
}

/// Two console modes, input then output, as little-endian `u32`.
pub(super) const STATE_SIZE: usize = 2 * std::mem::size_of::<u32>();

pub(super) fn get_state(
    input: BorrowedHandle<'_>,
    output: BorrowedHandle<'_>,
) -> AsyncHostResult<Vec<u8>> {
    let mut state = Vec::with_capacity(STATE_SIZE);
    state.extend_from_slice(&get_mode(input)?.to_le_bytes());
    state.extend_from_slice(&get_mode(output)?.to_le_bytes());
    Ok(state)
}

pub(super) fn set_state(
    input: BorrowedHandle<'_>,
    output: BorrowedHandle<'_>,
    state: &[u8],
) -> AsyncHostResult<()> {
    let state: &[u8; STATE_SIZE] = state.try_into().map_err(|_| AsyncHostError::Inval)?;
    let input_mode = u32::from_le_bytes(state[..4].try_into().unwrap());
    let output_mode = u32::from_le_bytes(state[4..].try_into().unwrap());
    let old_input_mode = get_mode(input)?;
    set_mode(input, input_mode)?;
    if let Err(error) = set_mode(output, output_mode) {
        // Preserve the output error while rolling back the input change, e.g.
        // on a legacy console that rejects the VT output flags.
        let _ = set_mode(input, old_input_mode);
        return Err(error);
    }
    Ok(())
}

pub(super) fn size(output: BorrowedHandle<'_>) -> AsyncHostResult<i64> {
    let mut info = unsafe { std::mem::zeroed::<CONSOLE_SCREEN_BUFFER_INFO>() };
    if unsafe { GetConsoleScreenBufferInfo(output.as_raw_handle(), &mut info) } == 0 {
        return Err(last_error());
    }
    // dwSize includes scrollback; srWindow describes the visible viewport.
    let rows = i64::from(info.srWindow.Bottom) - i64::from(info.srWindow.Top) + 1;
    let cols = i64::from(info.srWindow.Right) - i64::from(info.srWindow.Left) + 1;
    Ok((rows << 32) | cols)
}
