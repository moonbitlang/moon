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
}
