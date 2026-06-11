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
use crate::async_sys::internal::fd_util::stub::RawFd;
use crate::async_sys::ported_fns;

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/io_unix.c",
        original = "moonbitlang_async_read"
    )]
    #[allow(dead_code)]
    pub(crate) fn read(fd: RawFd, buf: &mut [u8], offset: i32, len: i32) -> AsyncHostResult<i32> {
        let range = checked_mut_range(buf, offset, len)?;
        let ret = unsafe { libc::read(fd, range.as_mut_ptr().cast(), range.len()) };
        if ret < 0 {
            Err(last_native_error())
        } else {
            i32::try_from(ret).map_err(|_| AsyncHostError::Fault)
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_unix.c",
        original = "moonbitlang_async_write"
    )]
    #[allow(dead_code)]
    pub(crate) fn write(fd: RawFd, buf: &[u8], offset: i32, len: i32) -> AsyncHostResult<i32> {
        let range = checked_range(buf, offset, len)?;
        let ret = unsafe { libc::write(fd, range.as_ptr().cast(), range.len()) };
        if ret < 0 {
            Err(last_native_error())
        } else {
            i32::try_from(ret).map_err(|_| AsyncHostError::Fault)
        }
    }
}

fn checked_range(buf: &[u8], offset: i32, len: i32) -> AsyncHostResult<&[u8]> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let end = offset.checked_add(len).ok_or(AsyncHostError::Fault)?;
    buf.get(offset..end).ok_or(AsyncHostError::Fault)
}

fn checked_mut_range(buf: &mut [u8], offset: i32, len: i32) -> AsyncHostResult<&mut [u8]> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let end = offset.checked_add(len).ok_or(AsyncHostError::Fault)?;
    buf.get_mut(offset..end).ok_or(AsyncHostError::Fault)
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
    fn read_and_write_use_buffer_offsets() {
        let mut fds = [0; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let read_fd = fds[0];
        let write_fd = fds[1];

        assert_eq!(write(write_fd, b"abcdef", 2, 3).unwrap(), 3);

        let mut buf = *b"------";
        assert_eq!(read(read_fd, &mut buf, 1, 3).unwrap(), 3);
        assert_eq!(&buf, b"-cde--");

        unsafe {
            libc::close(read_fd);
            libc::close(write_fd);
        }
    }
}
