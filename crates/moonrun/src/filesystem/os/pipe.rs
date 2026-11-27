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

//! Pipe creation and descriptor setup. Callers own publication and scheduling.

use std::fs::File;
use std::io;
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(windows)]
use std::os::windows::io::FromRawHandle;

use crate::upstream::ported_fns;

ported_fns! {
    #[ported(source = "src/internal/fd_util/stub.c", original = "moonbitlang_async_pipe")]
    #[cfg(unix)]
    pub(crate) fn pipe(read_end_is_async: bool, write_end_is_async: bool) -> io::Result<[File; 2]> {
        let mut fds = [0, 0];
        if unsafe { libc::pipe(fds.as_mut_ptr()) } < 0 {
            return Err(io::Error::last_os_error());
        }
        // Own both ends before configuring either, including all error paths.
        let files = fds.map(|fd| unsafe { File::from_raw_fd(fd) });
        for (file, is_async) in files.iter().zip([read_end_is_async, write_end_is_async]) {
            let fd = file.as_raw_fd();
            set_cloexec(fd)?;
            if is_async {
                let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
                if flags < 0 {
                    return Err(io::Error::last_os_error());
                }
                if flags & libc::O_NONBLOCK == 0
                    && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
        }
        Ok(files)
    }
}

#[cfg(unix)]
pub(crate) fn set_cloexec(fd: super::RawFile) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    if flags & libc::FD_CLOEXEC == 0
        && unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn pipe(read_end_is_async: bool, write_end_is_async: bool) -> io::Result<[File; 2]> {
    use std::sync::atomic::{AtomicU64, Ordering};
    use windows_sys::Win32::Foundation::{GENERIC_READ, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
        PIPE_ACCESS_OUTBOUND,
    };
    use windows_sys::Win32::System::Pipes::{
        CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };

    static PIPE_ID: AtomicU64 = AtomicU64::new(0);
    let pipe_id = PIPE_ID.fetch_add(1, Ordering::Relaxed) + 1;
    let name = format!(
        r"\\.\pipe\moonbitlang_async.{}.{}",
        std::process::id(),
        pipe_id
    )
    .encode_utf16()
    .chain([0])
    .collect::<Vec<_>>();
    let flags = PIPE_ACCESS_OUTBOUND
        | FILE_FLAG_FIRST_PIPE_INSTANCE
        | if write_end_is_async {
            FILE_FLAG_OVERLAPPED
        } else {
            0
        };
    let write = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            flags,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            1024,
            1024,
            0,
            std::ptr::null(),
        )
    };
    if write == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let write = unsafe { File::from_raw_handle(write) };
    let read = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            if read_end_is_async {
                FILE_FLAG_OVERLAPPED
            } else {
                0
            },
            std::ptr::null_mut(),
        )
    };
    if read == INVALID_HANDLE_VALUE {
        // Capture the client-open error before releasing the server endpoint.
        return Err(io::Error::last_os_error());
    }
    let read = unsafe { File::from_raw_handle(read) };
    Ok([read, write])
}
