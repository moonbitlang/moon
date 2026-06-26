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

use crate::async_host::AsyncHost;
use crate::async_sys::ported_fns;

ported_fns! {
    #[ported(
        source = "src/os_error/stub.c",
        original = "moonbitlang_async_get_errno"
    )]
    pub(crate) fn get_errno(host: &AsyncHost) -> i32 {
        host.get_errno()
    }

    #[ported(
        source = "src/os_error/stub.c",
        original = "moonbitlang_async_is_nonblocking_io_error"
    )]
    pub(crate) fn is_nonblocking_io_error(errno: i32) -> bool {
        #[cfg(unix)]
        {
            errno == libc::EAGAIN || errno == libc::EINPROGRESS || errno == libc::EWOULDBLOCK
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{ERROR_IO_INCOMPLETE, ERROR_IO_PENDING};
            errno == ERROR_IO_INCOMPLETE as i32 || errno == ERROR_IO_PENDING as i32
        }
    }

    #[ported(
        source = "src/os_error/stub.c",
        original = "moonbitlang_async_is_EINTR"
    )]
    pub(crate) fn is_eintr(errno: i32) -> bool {
        #[cfg(unix)]
        {
            errno == libc::EINTR
        }
        #[cfg(windows)]
        {
            let _ = errno;
            false
        }
    }

    #[ported(
        source = "src/os_error/stub.c",
        original = "moonbitlang_async_is_ENOENT"
    )]
    pub(crate) fn is_enoent(errno: i32) -> bool {
        #[cfg(unix)]
        {
            errno == libc::ENOENT
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
            errno == ERROR_FILE_NOT_FOUND as i32 || errno == ERROR_PATH_NOT_FOUND as i32
        }
    }

    #[ported(
        source = "src/os_error/stub.c",
        original = "moonbitlang_async_is_EEXIST"
    )]
    pub(crate) fn is_eexist(errno: i32) -> bool {
        #[cfg(unix)]
        {
            errno == libc::EEXIST
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS};
            errno == ERROR_FILE_EXISTS as i32 || errno == ERROR_ALREADY_EXISTS as i32
        }
    }

    #[ported(
        source = "src/os_error/stub.c",
        original = "moonbitlang_async_is_EACCES"
    )]
    pub(crate) fn is_eacces(errno: i32) -> bool {
        #[cfg(unix)]
        {
            errno == libc::EACCES
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
            errno == ERROR_ACCESS_DENIED as i32
        }
    }

    #[ported(
        source = "src/os_error/stub.c",
        original = "moonbitlang_async_is_ECONNREFUSED"
    )]
    pub(crate) fn is_econnrefused(errno: i32) -> bool {
        #[cfg(unix)]
        {
            errno == libc::ECONNREFUSED
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::ERROR_CONNECTION_REFUSED;
            errno == ERROR_CONNECTION_REFUSED as i32
        }
    }

    #[ported(
        source = "src/os_error/stub.c",
        original = "moonbitlang_async_is_ERROR_NOTIFY_ENUM_DIR"
    )]
    pub(crate) fn is_error_notify_enum_dir(errno: i32) -> bool {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::ERROR_NOTIFY_ENUM_DIR;
            errno == ERROR_NOTIFY_ENUM_DIR as i32
        }
        #[cfg(unix)]
        {
            let _ = errno;
            false
        }
    }

    #[ported(
        source = "src/os_error/stub.c",
        original = "moonbitlang_async_get_ENOTDIR"
    )]
    pub(crate) fn get_enotdir() -> i32 {
        #[cfg(unix)]
        {
            libc::ENOTDIR
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::ERROR_DIRECTORY;
            ERROR_DIRECTORY as i32
        }
    }

    #[ported(
        source = "src/os_error/stub.c",
        original = "moonbitlang_async_errno_to_string"
    )]
    pub(crate) fn errno_to_string(errno: i32) -> Box<[u8]> {
        errno_to_string_sys(errno)
    }
}

#[cfg(unix)]
fn errno_to_string_sys(errno: i32) -> Box<[u8]> {
    use std::ffi::CStr;

    let message = unsafe { libc::strerror(errno) };
    if message.is_null() {
        return fallback_errno_bytes(errno);
    }

    copy_c_string_bytes(unsafe { CStr::from_ptr(message) }.to_bytes())
}

#[cfg(windows)]
fn errno_to_string_sys(errno: i32) -> Box<[u8]> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::System::Diagnostics::Debug::{
        FORMAT_MESSAGE_ALLOCATE_BUFFER, FORMAT_MESSAGE_FROM_SYSTEM, FORMAT_MESSAGE_IGNORE_INSERTS,
        FormatMessageW,
    };

    let mut message = std::ptr::null_mut::<u16>();
    let len = unsafe {
        FormatMessageW(
            FORMAT_MESSAGE_ALLOCATE_BUFFER
                | FORMAT_MESSAGE_FROM_SYSTEM
                | FORMAT_MESSAGE_IGNORE_INSERTS,
            std::ptr::null(),
            errno as u32,
            0,
            std::ptr::addr_of_mut!(message).cast(),
            0,
            std::ptr::null(),
        )
    };
    if len == 0 {
        return fallback_errno_wide_bytes(errno);
    }

    let buffer = unsafe { std::slice::from_raw_parts(message, len as usize) };
    let bytes = copy_wide_c_string_bytes(buffer);
    unsafe {
        let _ = LocalFree(message.cast());
    }
    bytes
}

#[cfg(unix)]
fn fallback_errno_bytes(errno: i32) -> Box<[u8]> {
    let message = format!("errno {errno}");
    copy_c_string_bytes(message.as_bytes())
}

#[cfg(windows)]
fn fallback_errno_wide_bytes(errno: i32) -> Box<[u8]> {
    let message = format!("errno {errno}");
    let units = message.encode_utf16().collect::<Box<[_]>>();
    copy_wide_c_string_bytes(&units)
}

#[cfg(unix)]
fn copy_c_string_bytes(bytes: &[u8]) -> Box<[u8]> {
    let mut buffer = Box::<[u8]>::new_uninit_slice(bytes.len() + 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.as_mut_ptr().cast(), bytes.len());
    }
    buffer[bytes.len()].write(0);
    unsafe { buffer.assume_init() }
}

#[cfg(windows)]
fn copy_wide_c_string_bytes(units: &[u16]) -> Box<[u8]> {
    let byte_len = std::mem::size_of_val(units);
    let mut buffer = Box::<[u8]>::new_uninit_slice(byte_len + std::mem::size_of::<u16>());
    unsafe {
        std::ptr::copy_nonoverlapping(
            units.as_ptr().cast::<u8>(),
            buffer.as_mut_ptr().cast(),
            byte_len,
        );
    }
    buffer[byte_len].write(0);
    buffer[byte_len + 1].write(0);
    unsafe { buffer.assume_init() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errno_to_string_returns_native_string_buffer() {
        let buffer = errno_to_string(get_enotdir());
        assert!(!buffer.is_empty());

        #[cfg(unix)]
        {
            assert_eq!(buffer.last(), Some(&0));
            assert!(buffer.len() > 1);
        }

        #[cfg(windows)]
        {
            assert!(buffer.len().is_multiple_of(std::mem::size_of::<u16>()));
            assert_eq!(
                buffer.get(buffer.len().saturating_sub(2)..),
                Some(&[0, 0][..])
            );
        }
    }
}
