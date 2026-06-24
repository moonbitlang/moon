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

#[cfg(unix)]
use crate::async_host::{AsyncHostError, AsyncHostResult};
#[cfg(unix)]
use crate::async_sys::internal::fd_util::stub::RawFd;
use crate::async_sys::ported_fns;

#[cfg(unix)]
fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/io_unix.c",
        original = "moonbitlang_async_read"
    )]
    #[cfg(unix)]
    pub(crate) fn read(fd: RawFd, buf: &mut [u8]) -> AsyncHostResult<usize> {
        let ret = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if ret < 0 {
            Err(last_native_error())
        } else {
            usize::try_from(ret).map_err(|_| AsyncHostError::Fault)
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_unix.c",
        original = "moonbitlang_async_write"
    )]
    #[cfg(unix)]
    pub(crate) fn write(fd: RawFd, buf: &[u8]) -> AsyncHostResult<usize> {
        let ret = unsafe { libc::write(fd, buf.as_ptr().cast(), buf.len()) };
        if ret < 0 {
            Err(last_native_error())
        } else {
            usize::try_from(ret).map_err(|_| AsyncHostError::Fault)
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_errno_is_read_EOF"
    )]
    #[cfg(windows)]
    pub(crate) fn errno_is_read_eof(errno: i32) -> bool {
        use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF};

        errno == ERROR_HANDLE_EOF as i32 || errno == ERROR_BROKEN_PIPE as i32
    }
}

#[cfg(test)]
pub(crate) fn ported_symbols() -> Vec<crate::async_sys::PortedSymbol> {
    #[cfg(not(windows))]
    {
        PORTED_SYMBOLS.to_vec()
    }

    #[cfg(windows)]
    {
        let mut symbols = PORTED_SYMBOLS.to_vec();
        symbols.extend_from_slice(&[
            ported_windows_symbol(
                "make_file_io_result",
                "moonbitlang_async_make_file_io_result",
            ),
            ported_windows_symbol("free_io_result", "moonbitlang_async_free_io_result"),
            ported_windows_symbol(
                "io_result_get_event",
                "moonbitlang_async_io_result_get_event",
            ),
            ported_windows_symbol("cancel_io_result", "moonbitlang_async_cancel_io_result"),
            ported_windows_symbol(
                "io_result_get_status",
                "moonbitlang_async_io_result_get_status",
            ),
            ported_windows_symbol("read_io_result", "moonbitlang_async_read"),
            ported_windows_symbol("write_io_result", "moonbitlang_async_write"),
        ]);
        symbols
    }
}

#[cfg(all(test, windows))]
fn ported_windows_symbol(
    rust_symbol: &'static str,
    native_symbol: &'static str,
) -> crate::async_sys::PortedSymbol {
    crate::async_sys::PortedSymbol {
        rust_module: module_path!(),
        rust_symbol,
        native_symbol,
        source: "src/internal/event_loop/io_windows.c",
    }
}
