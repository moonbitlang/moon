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

//! Windows console I/O from native thread_pool.c. Callers own the UTF-16 buffers
//! and bracket each native call with their execution/cancellation mechanism.

use super::{BlockingIo, RawFile};
use std::io;
use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF, ERROR_INVALID_ADDRESS};
use windows_sys::Win32::System::Console::{ReadConsoleW, WriteConsoleW};

pub(in crate::filesystem) fn read(
    handle: RawFile,
    buffer: &mut [u16],
    blocking: &impl BlockingIo,
) -> io::Result<usize> {
    let len = u32::try_from(buffer.len())
        .map_err(|_| io::Error::from_raw_os_error(ERROR_INVALID_ADDRESS as i32))?;
    blocking.run(|| {
        let mut count = 0;
        if unsafe {
            ReadConsoleW(
                handle,
                buffer.as_mut_ptr().cast(),
                len,
                &mut count,
                std::ptr::null(),
            )
        } != 0
        {
            Ok(count as usize)
        } else {
            // Capture the native error before leaving the execution bracket.
            let error = io::Error::last_os_error();
            match error.raw_os_error() {
                Some(code)
                    if code == ERROR_HANDLE_EOF as i32 || code == ERROR_BROKEN_PIPE as i32 =>
                {
                    Ok(0)
                }
                _ => Err(error),
            }
        }
    })
}

pub(in crate::filesystem) fn write(
    handle: RawFile,
    buffer: &[u16],
    blocking: &impl BlockingIo,
) -> io::Result<usize> {
    let len = u32::try_from(buffer.len())
        .map_err(|_| io::Error::from_raw_os_error(ERROR_INVALID_ADDRESS as i32))?;
    blocking.run(|| {
        let mut count = 0;
        if unsafe {
            WriteConsoleW(
                handle,
                buffer.as_ptr().cast(),
                len,
                &mut count,
                std::ptr::null(),
            )
        } != 0
        {
            Ok(count as usize)
        } else {
            Err(io::Error::last_os_error())
        }
    })
}
