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

#[cfg(unix)]
pub(crate) type RawFd = std::os::fd::RawFd;

#[cfg(windows)]
pub(crate) type RawFd = windows_sys::Win32::Foundation::HANDLE;

#[cfg(windows)]
pub(crate) type FileTime = windows_sys::Win32::Storage::FileSystem::FILE_BASIC_INFO;

#[cfg(unix)]
pub(crate) type FileTime = libc::stat;

#[cfg(windows)]
const WINDOWS_TICKS_PER_SECOND: i64 = 10_000_000;
#[cfg(windows)]
const WINDOWS_TO_UNIX_EPOCH_SECONDS: i64 = 11_644_473_600;

#[cfg(windows)]
fn windows_filetime_to_unix_seconds(ticks: i64) -> i64 {
    ticks / WINDOWS_TICKS_PER_SECOND - WINDOWS_TO_UNIX_EPOCH_SECONDS
}

#[cfg(windows)]
fn windows_filetime_to_nanoseconds(ticks: i64) -> i32 {
    ((ticks % WINDOWS_TICKS_PER_SECOND) * 100) as i32
}

ported_fns! {
    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_get_invalid_handle"
    )]
    #[cfg(windows)]
    #[allow(dead_code)]
    pub(crate) fn get_invalid_handle() -> RawFd {
        windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_close_fd"
    )]
    #[cfg(windows)]
    #[allow(dead_code)]
    pub(crate) fn close_fd(fd: RawFd, is_socket: bool) -> AsyncHostResult<()> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::Networking::WinSock::{SOCKET, closesocket};

        let ok = if is_socket {
            unsafe { closesocket(fd as SOCKET) == 0 }
        } else {
            unsafe { CloseHandle(fd) != 0 }
        };
        if ok { Ok(()) } else { Err(last_native_error()) }
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_fd_is_nonblocking"
    )]
    #[cfg(unix)]
    #[allow(dead_code)]
    pub(crate) fn fd_is_nonblocking(fd: RawFd) -> AsyncHostResult<bool> {
        let flags = fcntl_getfl(fd)?;
        Ok((flags & libc::O_NONBLOCK) > 0)
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_set_blocking"
    )]
    #[cfg(unix)]
    #[allow(dead_code)]
    pub(crate) fn set_blocking(fd: RawFd) -> AsyncHostResult<()> {
        let flags = fcntl_getfl(fd)?;
        if (flags & libc::O_NONBLOCK) != 0 {
            fcntl_setfl(fd, flags & !libc::O_NONBLOCK)?;
        }
        Ok(())
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_set_nonblocking"
    )]
    #[cfg(unix)]
    #[allow(dead_code)]
    pub(crate) fn set_nonblocking(fd: RawFd) -> AsyncHostResult<()> {
        let flags = fcntl_getfl(fd)?;
        if (flags & libc::O_NONBLOCK) == 0 {
            fcntl_setfl(fd, flags | libc::O_NONBLOCK)?;
        }
        Ok(())
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_set_cloexec"
    )]
    #[cfg(unix)]
    #[allow(dead_code)]
    pub(crate) fn set_cloexec(fd: RawFd) -> AsyncHostResult<()> {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if flags < 0 {
            return Err(last_native_error());
        }
        if (flags & libc::FD_CLOEXEC) == 0 {
            let ret = unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) };
            if ret < 0 {
                return Err(last_native_error());
            }
        }
        Ok(())
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_create_named_pipe_server"
    )]
    #[cfg(windows)]
    #[allow(dead_code)]
    pub(crate) fn create_named_pipe_server(name: &std::ffi::OsStr, is_async: bool) -> RawFd {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_OUTBOUND,
        };
        use windows_sys::Win32::System::Pipes::{
            CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
            PIPE_WAIT,
        };

        let mut name: Vec<u16> = name.encode_wide().collect();
        name.push(0);
        let flags = PIPE_ACCESS_OUTBOUND
            | FILE_FLAG_FIRST_PIPE_INSTANCE
            | if is_async { FILE_FLAG_OVERLAPPED } else { 0 };
        unsafe {
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
        }
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_create_named_pipe_client"
    )]
    #[cfg(windows)]
    #[allow(dead_code)]
    pub(crate) fn create_named_pipe_client(name: &std::ffi::OsStr, is_async: bool) -> RawFd {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::GENERIC_READ;
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
        };

        let mut name: Vec<u16> = name.encode_wide().collect();
        name.push(0);
        unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                if is_async { FILE_FLAG_OVERLAPPED } else { 0 },
                std::ptr::null_mut(),
            )
        }
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_pipe"
    )]
    #[cfg(unix)]
    #[allow(dead_code)]
    pub(crate) fn pipe() -> AsyncHostResult<[RawFd; 2]> {
        let mut fds = [0, 0];
        if unsafe { libc::pipe(fds.as_mut_ptr()) } < 0 {
            return Err(last_native_error());
        }
        for fd in fds {
            if let Err(error) = set_cloexec(fd) {
                unsafe {
                    libc::close(fds[0]);
                    libc::close(fds[1]);
                }
                return Err(error);
            }
        }
        Ok(fds)
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_sizeof_file_time"
    )]
    #[allow(dead_code)]
    pub(crate) fn sizeof_file_time() -> i32 {
        std::mem::size_of::<FileTime>() as i32
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_get_atime_sec"
    )]
    #[allow(dead_code)]
    #[allow(clippy::unnecessary_cast)]
    pub(crate) fn get_atime_sec(file_time: &FileTime) -> i64 {
        #[cfg(windows)]
        {
            windows_filetime_to_unix_seconds(file_time.LastAccessTime)
        }
        #[cfg(unix)]
        {
            file_time.st_atime as i64
        }
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_get_atime_nsec"
    )]
    #[allow(dead_code)]
    pub(crate) fn get_atime_nsec(file_time: &FileTime) -> i32 {
        #[cfg(windows)]
        {
            windows_filetime_to_nanoseconds(file_time.LastAccessTime)
        }
        #[cfg(unix)]
        {
            file_time.st_atime_nsec as i32
        }
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_get_mtime_sec"
    )]
    #[allow(dead_code)]
    #[allow(clippy::unnecessary_cast)]
    pub(crate) fn get_mtime_sec(file_time: &FileTime) -> i64 {
        #[cfg(windows)]
        {
            windows_filetime_to_unix_seconds(file_time.LastWriteTime)
        }
        #[cfg(unix)]
        {
            file_time.st_mtime as i64
        }
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_get_mtime_nsec"
    )]
    #[allow(dead_code)]
    pub(crate) fn get_mtime_nsec(file_time: &FileTime) -> i32 {
        #[cfg(windows)]
        {
            windows_filetime_to_nanoseconds(file_time.LastWriteTime)
        }
        #[cfg(unix)]
        {
            file_time.st_mtime_nsec as i32
        }
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_get_ctime_sec"
    )]
    #[allow(dead_code)]
    #[allow(clippy::unnecessary_cast)]
    pub(crate) fn get_ctime_sec(file_time: &FileTime) -> i64 {
        #[cfg(windows)]
        {
            windows_filetime_to_unix_seconds(file_time.ChangeTime)
        }
        #[cfg(unix)]
        {
            file_time.st_ctime as i64
        }
    }

    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_get_ctime_nsec"
    )]
    #[allow(dead_code)]
    pub(crate) fn get_ctime_nsec(file_time: &FileTime) -> i32 {
        #[cfg(windows)]
        {
            windows_filetime_to_nanoseconds(file_time.ChangeTime)
        }
        #[cfg(unix)]
        {
            file_time.st_ctime_nsec as i32
        }
    }
}

#[cfg(unix)]
fn fcntl_getfl(fd: RawFd) -> AsyncHostResult<i32> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        Err(last_native_error())
    } else {
        Ok(flags)
    }
}

#[cfg(unix)]
fn fcntl_setfl(fd: RawFd, flags: i32) -> AsyncHostResult<()> {
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizeof_file_time_matches_platform_stat_buffer() {
        assert_eq!(sizeof_file_time(), std::mem::size_of::<FileTime>() as i32);
    }

    #[cfg(windows)]
    #[test]
    fn windows_filetime_seconds_use_unix_epoch() {
        let unix_epoch = WINDOWS_TO_UNIX_EPOCH_SECONDS * WINDOWS_TICKS_PER_SECOND;

        assert_eq!(windows_filetime_to_unix_seconds(unix_epoch), 0);
        assert_eq!(
            windows_filetime_to_unix_seconds(unix_epoch + WINDOWS_TICKS_PER_SECOND),
            1
        );
        assert_eq!(windows_filetime_to_nanoseconds(unix_epoch + 123), 12_300);
    }

    #[cfg(unix)]
    #[test]
    fn unix_set_nonblocking_and_blocking_match_native_stub() {
        let fds = pipe().unwrap();

        assert!(!fd_is_nonblocking(fds[0]).unwrap());
        set_nonblocking(fds[0]).unwrap();
        assert!(fd_is_nonblocking(fds[0]).unwrap());
        set_blocking(fds[0]).unwrap();
        assert!(!fd_is_nonblocking(fds[0]).unwrap());

        unsafe {
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
    }
}
