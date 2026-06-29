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

use super::context::ImportContext;
use super::provenance::ported_imports;

ported_imports! {
#[ported(source = "src/internal/event_loop/io_unix.c")]
#[cfg(unix)]
pub(super) fn read(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    dst: i32,
    offset: i32,
    len: i32,
) -> i32 {
    // Unix io/read is the pollable nonblocking path. Regular files are routed
    // through worker jobs, so this borrowed guest slice must not outlive the syscall.
    match context
        .with_host_and_memory_mut(|host, memory| host.read_fd(memory, fd, dst, offset, len))
    {
        Ok(bytes) => bytes,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(source = "src/internal/event_loop/io_unix.c")]
#[cfg(unix)]
pub(super) fn write(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    src: i32,
    offset: i32,
    len: i32,
) -> i32 {
    // See io/read: this import is for pollable nonblocking handles, not
    // regular files, and the borrowed guest slice is used only for the syscall.
    match context
        .with_host_and_memory_mut(|host, memory| host.write_fd(memory, fd, src, offset, len))
    {
        Ok(bytes) => bytes,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_make_file_io_result"
)]
#[cfg(windows)]
pub(super) fn make_file_io_result(
    context: &mut ImportContext<'_, '_>,
    events: i32,
    buf: i32,
    offset: i32,
    len: i32,
    position: i64,
) -> crate::async_host::AsyncHostResult<u64> {
    context.with_host_and_memory_mut(|host, memory| {
        host.make_file_io_result(memory, events, buf, offset, len, position)
    })
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_make_socket_io_result"
)]
#[cfg(windows)]
pub(super) fn make_socket_io_result(
    context: &mut ImportContext<'_, '_>,
    events: i32,
    buf: i32,
    offset: i32,
    len: i32,
    flags: i32,
) -> crate::async_host::AsyncHostResult<u64> {
    context.with_host_and_memory_mut(|host, memory| {
        host.make_socket_io_result(memory, events, buf, offset, len, flags)
    })
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_make_socket_with_addr_io_result"
)]
#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
pub(super) fn make_socket_with_addr_io_result(
    context: &mut ImportContext<'_, '_>,
    events: i32,
    buf: i32,
    offset: i32,
    len: i32,
    flags: i32,
    addr: i32,
    addr_len: i32,
) -> crate::async_host::AsyncHostResult<u64> {
    context.with_host_and_memory_mut(|host, memory| {
        host.make_socket_with_addr_io_result(memory, events, buf, offset, len, flags, addr, addr_len)
    })
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_make_connect_io_result"
)]
#[cfg(windows)]
pub(super) fn make_connect_io_result(
    context: &mut ImportContext<'_, '_>,
    addr: i32,
    addr_len: i32,
) -> crate::async_host::AsyncHostResult<u64> {
    context.with_host_and_memory_mut(|host, memory| {
        host.make_connect_io_result(memory, addr, addr_len)
    })
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_make_accept_io_result"
)]
#[cfg(windows)]
pub(super) fn make_accept_io_result(
    context: &mut ImportContext<'_, '_>,
) -> crate::async_host::AsyncHostResult<u64> {
    context.host.make_accept_io_result()
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_free_io_result"
)]
#[cfg(windows)]
pub(super) fn free_io_result(
    context: &mut ImportContext<'_, '_>,
    result: u64,
) -> crate::async_host::AsyncHostResult<()> {
    context.host.free_io_result(result)
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_io_result_get_event"
)]
#[cfg(windows)]
pub(super) fn io_result_get_event(
    context: &mut ImportContext<'_, '_>,
    result: u64,
) -> crate::async_host::AsyncHostResult<i32> {
    context.host.io_result_get_event(result)
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_cancel_io_result"
)]
#[cfg(windows)]
pub(super) fn cancel_io_result(context: &mut ImportContext<'_, '_>, result: u64, fd: u64) -> i32 {
    match context.host.cancel_io_result(result, fd) {
        Ok(status) => status,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_io_result_get_status"
)]
#[cfg(windows)]
pub(super) fn io_result_get_status(context: &mut ImportContext<'_, '_>, result: u64, fd: u64) -> i32 {
    match context
        .with_host_and_memory_mut(|host, memory| host.io_result_get_status(memory, result, fd))
    {
        Ok(bytes) => bytes,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_read"
)]
#[cfg(windows)]
pub(super) fn read_io_result(context: &mut ImportContext<'_, '_>, fd: u64, result: u64) -> i32 {
    match context.with_host_and_memory_mut(|host, memory| host.read_io_result(memory, fd, result)) {
        Ok(bytes) => bytes,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_write"
)]
#[cfg(windows)]
pub(super) fn write_io_result(context: &mut ImportContext<'_, '_>, fd: u64, result: u64) -> i32 {
    match context.with_host_and_memory_mut(|host, memory| host.write_io_result(memory, fd, result))
    {
        Ok(bytes) => bytes,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_errno_is_read_EOF"
)]
#[cfg(windows)]
pub(super) fn errno_is_read_eof(_context: &mut ImportContext<'_, '_>, errno: i32) -> i32 {
    if crate::async_sys::internal::event_loop::io::errno_is_read_eof(errno) {
        1
    } else {
        0
    }
}
}
