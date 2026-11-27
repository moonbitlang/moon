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

//! Native file I/O. Callers own authorization, buffers, and completion state.
//! Worker reads/writes preserve the mechanisms in native thread_pool.c; the
//! supplied BlockingIo brackets individual syscalls without owning scheduling.

use std::io;

use super::{BlockingIo, RawFile};
use crate::upstream::ported_fns;

ported_fns! {
    #[ported(source = "src/internal/event_loop/io_unix.c", original = "moonbitlang_async_read")]
    #[cfg(unix)]
    pub(in crate::filesystem) fn read(file: RawFile, buffer: &mut [u8]) -> io::Result<usize> {
        let result = unsafe { libc::read(file, buffer.as_mut_ptr().cast(), buffer.len()) };
        if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(result as usize)
        }
    }

    #[ported(source = "src/internal/event_loop/io_unix.c", original = "moonbitlang_async_write")]
    #[cfg(unix)]
    pub(in crate::filesystem) fn write(file: RawFile, buffer: &[u8]) -> io::Result<usize> {
        let result = unsafe { libc::write(file, buffer.as_ptr().cast(), buffer.len()) };
        if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(result as usize)
        }
    }
}

#[cfg(unix)]
pub(in crate::filesystem) fn read_blocking(
    file: RawFile,
    buffer: &mut [u8],
    position: Option<u64>,
    blocking: &impl BlockingIo,
) -> io::Result<usize> {
    if let Some(position) = position {
        // Native async waits for positioned Unix I/O without cancellation.
        let result = unsafe {
            libc::pread(
                file,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                position as libc::off_t,
            )
        };
        return if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(result as usize)
        };
    }
    loop {
        match blocking.run(|| read(file, buffer)) {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                blocking.run(|| wait_ready(file, libc::POLLIN))?;
            }
            result => return result,
        }
    }
}

#[cfg(unix)]
pub(in crate::filesystem) fn write_blocking(
    file: RawFile,
    buffer: &[u8],
    position: Option<u64>,
    blocking: &impl BlockingIo,
) -> io::Result<usize> {
    if let Some(position) = position {
        let result = unsafe {
            libc::pwrite(
                file,
                buffer.as_ptr().cast(),
                buffer.len(),
                position as libc::off_t,
            )
        };
        return if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(result as usize)
        };
    }
    loop {
        match blocking.run(|| write(file, buffer)) {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                blocking.run(|| wait_ready(file, libc::POLLOUT))?;
            }
            result => return result,
        }
    }
}

#[cfg(unix)]
fn wait_ready(file: RawFile, events: libc::c_short) -> io::Result<()> {
    let mut pollfd = libc::pollfd {
        fd: file,
        events,
        revents: 0,
    };
    if unsafe { libc::poll(&mut pollfd, 1, -1) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Initiate one native read, capturing its error before returning to the caller.
///
/// # Safety
/// With an asynchronous handle, the caller must retain the file, buffer, and
/// OVERLAPPED through completion and prevent buffer access while I/O is pending.
#[cfg(windows)]
pub(in crate::filesystem) unsafe fn read(
    file: RawFile,
    buffer: &mut [u8],
    overlapped: *mut windows_sys::Win32::System::IO::OVERLAPPED,
) -> io::Result<usize> {
    use windows_sys::Win32::Foundation::ERROR_INVALID_ADDRESS;
    use windows_sys::Win32::Storage::FileSystem::ReadFile;

    let len = u32::try_from(buffer.len())
        .map_err(|_| io::Error::from_raw_os_error(ERROR_INVALID_ADDRESS as i32))?;
    let mut transferred = 0;
    if unsafe {
        ReadFile(
            file,
            buffer.as_mut_ptr().cast(),
            len,
            &mut transferred,
            overlapped,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(transferred as usize)
    }
}

/// Initiate one native write, capturing its error before returning to the caller.
///
/// # Safety
/// With an asynchronous handle, the caller must retain the file, buffer, and
/// OVERLAPPED through completion and prevent buffer mutation while I/O is pending.
#[cfg(windows)]
pub(in crate::filesystem) unsafe fn write(
    file: RawFile,
    buffer: &[u8],
    overlapped: *mut windows_sys::Win32::System::IO::OVERLAPPED,
) -> io::Result<usize> {
    use windows_sys::Win32::Foundation::ERROR_INVALID_ADDRESS;
    use windows_sys::Win32::Storage::FileSystem::WriteFile;

    let len = u32::try_from(buffer.len())
        .map_err(|_| io::Error::from_raw_os_error(ERROR_INVALID_ADDRESS as i32))?;
    let mut transferred = 0;
    if unsafe {
        WriteFile(
            file,
            buffer.as_ptr().cast(),
            len,
            &mut transferred,
            overlapped,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(transferred as usize)
    }
}

#[cfg(windows)]
pub(in crate::filesystem) fn read_blocking(
    file: RawFile,
    buffer: &mut [u8],
    position: Option<u64>,
    blocking: &impl BlockingIo,
) -> io::Result<usize> {
    use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF};
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    let overlapped_ptr = if let Some(position) = position {
        overlapped.Anonymous.Anonymous.Offset = position as u32;
        overlapped.Anonymous.Anonymous.OffsetHigh = (position >> 32) as u32;
        &mut overlapped
    } else {
        std::ptr::null_mut()
    };
    // Synchronous Windows handles advance the file pointer even with an offset.
    // Preserve positioned I/O semantics for subsequent stream reads/writes.
    let saved_position = position.map(|_| current_file_pointer(file)).transpose()?;
    // Keep cancellation failure separate from a completed syscall's error so
    // cancellation retains its existing early-return behavior.
    let result = blocking.run(|| Ok(unsafe { read(file, buffer, overlapped_ptr) }))?;
    let result = match result {
        Err(error)
            if matches!(error.raw_os_error(), Some(code) if
            code == ERROR_HANDLE_EOF as i32 || code == ERROR_BROKEN_PIPE as i32) =>
        {
            Ok(0)
        }
        result => result,
    };
    if let Some(saved_position) = saved_position
        && let Err(error) = restore_file_pointer(file, saved_position)
    {
        return result.and(Err(error));
    }
    result
}

#[cfg(windows)]
pub(in crate::filesystem) fn write_blocking(
    file: RawFile,
    buffer: &[u8],
    position: Option<u64>,
    blocking: &impl BlockingIo,
) -> io::Result<usize> {
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    let overlapped_ptr = if let Some(position) = position {
        overlapped.Anonymous.Anonymous.Offset = position as u32;
        overlapped.Anonymous.Anonymous.OffsetHigh = (position >> 32) as u32;
        &mut overlapped
    } else {
        std::ptr::null_mut()
    };
    let saved_position = position.map(|_| current_file_pointer(file)).transpose()?;
    let result = blocking.run(|| Ok(unsafe { write(file, buffer, overlapped_ptr) }))?;
    if let Some(saved_position) = saved_position
        && let Err(error) = restore_file_pointer(file, saved_position)
    {
        return result.and(Err(error));
    }
    result
}

#[cfg(windows)]
fn current_file_pointer(file: RawFile) -> io::Result<i64> {
    use windows_sys::Win32::Storage::FileSystem::{FILE_CURRENT, SetFilePointerEx};

    let mut current = 0;
    if unsafe { SetFilePointerEx(file, 0, &mut current, FILE_CURRENT) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(current)
    }
}

#[cfg(windows)]
fn restore_file_pointer(file: RawFile, position: i64) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{FILE_BEGIN, SetFilePointerEx};

    if unsafe { SetFilePointerEx(file, position, std::ptr::null_mut(), FILE_BEGIN) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
pub(in crate::filesystem) fn sync(fd: RawFile, only_data: bool) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    let ret = {
        let _ = only_data;
        unsafe { libc::fsync(fd) }
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let ret = unsafe {
        if only_data {
            libc::fdatasync(fd)
        } else {
            libc::fsync(fd)
        }
    };

    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub(in crate::filesystem) fn sync(file: RawFile, _only_data: bool) -> io::Result<()> {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::FlushFileBuffers;

    if unsafe { FlushFileBuffers(file as HANDLE) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::os::{DirectIo, raw_file};
    use std::io::{Read, Seek, SeekFrom, Write};

    #[test]
    fn positioned_io_preserves_the_stream_position() {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(b"abcdef").unwrap();
        file.seek(SeekFrom::Start(1)).unwrap();
        let raw = raw_file(&file);

        let mut buffer = [0; 2];
        assert_eq!(
            read_blocking(raw, &mut buffer, Some(4), &DirectIo).unwrap(),
            2
        );
        assert_eq!(&buffer, b"ef");
        assert_eq!(file.stream_position().unwrap(), 1);
        assert_eq!(write_blocking(raw, b"XY", Some(3), &DirectIo).unwrap(), 2);
        assert_eq!(file.stream_position().unwrap(), 1);

        assert_eq!(
            read_blocking(raw, &mut buffer[..1], None, &DirectIo).unwrap(),
            1
        );
        assert_eq!(buffer[0], b'b');
        assert_eq!(write_blocking(raw, b"Z", None, &DirectIo).unwrap(), 1);
        assert_eq!(file.stream_position().unwrap(), 3);
        assert_eq!(
            read_blocking(raw, &mut buffer, Some(6), &DirectIo).unwrap(),
            0
        );
        assert_eq!(file.stream_position().unwrap(), 3);

        file.rewind().unwrap();
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).unwrap();
        assert_eq!(contents, b"abZXYf");
    }

    #[cfg(unix)]
    #[test]
    fn immediate_io_preserves_would_block_partial_reads_and_eof() {
        use std::os::fd::AsRawFd;
        use std::os::unix::net::UnixStream;

        let (reader, writer) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        let mut buffer = [0; 4];
        assert_eq!(
            read(reader.as_raw_fd(), &mut buffer).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(write(writer.as_raw_fd(), b"hi").unwrap(), 2);
        assert_eq!(read(reader.as_raw_fd(), &mut buffer).unwrap(), 2);
        assert_eq!(&buffer[..2], b"hi");
        drop(writer);
        assert_eq!(read(reader.as_raw_fd(), &mut buffer).unwrap(), 0);
    }
}
