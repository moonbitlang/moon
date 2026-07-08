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

use crate::async_host::{AsyncHostError, AsyncHostResult, GuestMemory};

use super::context::ImportContext;
use super::provenance::ported_imports;

ported_imports! {
const FILE_TIME_RECORD_LEN: i32 = 48;
const ATIME_SEC_OFFSET: i32 = 0;
const ATIME_NSEC_OFFSET: i32 = 8;
const MTIME_SEC_OFFSET: i32 = 16;
const MTIME_NSEC_OFFSET: i32 = 24;
const CTIME_SEC_OFFSET: i32 = 32;
const CTIME_NSEC_OFFSET: i32 = 40;

pub(super) fn invalid_fd(context: &mut ImportContext<'_, '_>) -> u64 {
    context.host.invalid_fd()
}

#[ported(
    source = "src/internal/event_loop/detect_file_kind.c",
    original = "moonbitlang_async_kind_of_fd"
)]
pub(super) fn kind_of_fd(context: &mut ImportContext<'_, '_>, fd: u64) -> i32 {
    match context.host.kind_of_fd(fd) {
        Ok(kind) => kind,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

pub(super) fn sizeof_file_time(_context: &mut ImportContext<'_, '_>) -> i32 {
    FILE_TIME_RECORD_LEN
}

#[ported(source = "src/internal/fd_util/stub.c")]
pub(super) fn get_atime_sec(context: &mut ImportContext<'_, '_>, ptr: i32) -> AsyncHostResult<i64> {
    file_time_i64(context, ptr, ATIME_SEC_OFFSET)
}

#[ported(source = "src/internal/fd_util/stub.c")]
pub(super) fn get_atime_nsec(context: &mut ImportContext<'_, '_>, ptr: i32) -> AsyncHostResult<i32> {
    file_time_i32(context, ptr, ATIME_NSEC_OFFSET)
}

#[ported(source = "src/internal/fd_util/stub.c")]
pub(super) fn get_mtime_sec(context: &mut ImportContext<'_, '_>, ptr: i32) -> AsyncHostResult<i64> {
    file_time_i64(context, ptr, MTIME_SEC_OFFSET)
}

#[ported(source = "src/internal/fd_util/stub.c")]
pub(super) fn get_mtime_nsec(context: &mut ImportContext<'_, '_>, ptr: i32) -> AsyncHostResult<i32> {
    file_time_i32(context, ptr, MTIME_NSEC_OFFSET)
}

#[ported(source = "src/internal/fd_util/stub.c")]
pub(super) fn get_ctime_sec(context: &mut ImportContext<'_, '_>, ptr: i32) -> AsyncHostResult<i64> {
    file_time_i64(context, ptr, CTIME_SEC_OFFSET)
}

#[ported(source = "src/internal/fd_util/stub.c")]
pub(super) fn get_ctime_nsec(context: &mut ImportContext<'_, '_>, ptr: i32) -> AsyncHostResult<i32> {
    file_time_i32(context, ptr, CTIME_NSEC_OFFSET)
}

#[ported(source = "src/internal/fd_util/stub.c")]
pub(super) fn pipe(
    context: &mut ImportContext<'_, '_>,
    dst: i32,
    len: i32,
    read_end_is_async: i32,
    write_end_is_async: i32,
) -> i32 {
    let result = (|| {
        if len < 2 {
            return Err(AsyncHostError::Fault);
        }
        context.with_memory_mut(|memory| memory.read_exact_mut(dst, 16).map(|_| ()))?;
        let fds = context.host
            .pipe(read_end_is_async != 0, write_end_is_async != 0)?;
        context.with_memory_mut(|memory| {
            memory.write_u64_le(dst, fds[0])?;
            memory.write_u64_le(dst.checked_add(8).ok_or(AsyncHostError::Fault)?, fds[1])
        })
    })();
    match result {
        Ok(()) => 0,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

// Deprecated. We'll remove it later.
pub(super) fn set_nonblocking(_context: &mut ImportContext<'_, '_>, _fd: u64) -> i32 {
    0
}

pub(super) fn set_cloexec(context: &mut ImportContext<'_, '_>, fd: u64) -> i32 {
    match context.host.set_cloexec(fd) {
        Ok(()) => 0,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

fn file_time_i64(context: &mut ImportContext<'_, '_>, ptr: i32, field_offset: i32) -> AsyncHostResult<i64> {
    read_field(context, ptr, field_offset, 8)
        .map(|bytes| i64::from_le_bytes(bytes.as_slice().try_into().unwrap()))
}

fn file_time_i32(context: &mut ImportContext<'_, '_>, ptr: i32, field_offset: i32) -> AsyncHostResult<i32> {
    read_field(context, ptr, field_offset, 4)
        .map(|bytes| i32::from_le_bytes(bytes.as_slice().try_into().unwrap()))
}

fn read_field(
    context: &mut ImportContext<'_, '_>,
    ptr: i32,
    field_offset: i32,
    len: i32,
) -> AsyncHostResult<Vec<u8>> {
    let offset = ptr.checked_add(field_offset).ok_or(AsyncHostError::Fault)?;
    context.with_memory_mut(|memory| Ok(memory.read_exact(offset, len)?.to_vec()))
}
}
