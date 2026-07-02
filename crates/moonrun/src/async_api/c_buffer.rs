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
use crate::async_sys::internal::c_buffer::stub;

use super::context::ImportContext;
use super::provenance::ported_imports;

ported_imports! {
pub(super) fn is_null(_context: &mut ImportContext<'_, '_>, ptr: u64) -> i32 {
    i32::from(ptr == crate::async_host::INVALID_HOST_HANDLE)
}

#[ported(source = "src/internal/c_buffer/stub.c")]
pub(super) fn blit_to_c(
    context: &mut ImportContext<'_, '_>,
    dst: u64,
    dst_offset: i32,
    src: i32,
    src_offset: i32,
    len: i32,
) -> AsyncHostResult<()> {
    let src_len = checked_add_i32(src_offset, len)?;
    context.with_host_and_memory_mut(|host, memory| {
        let src = memory.read_exact(src, src_len)?;
        host.with_c_buffer_mut(dst, |dst| {
            stub::blit_to_c(dst, dst_offset, src, src_offset, len)
        })
    })
}

#[ported(source = "src/internal/c_buffer/stub.c")]
pub(super) fn blit_from_c(
    context: &mut ImportContext<'_, '_>,
    src: u64,
    src_offset: i32,
    dst: i32,
    dst_offset: i32,
    len: i32,
) -> AsyncHostResult<()> {
    let dst_len = checked_add_i32(dst_offset, len)?;
    context.with_host_and_memory_mut(|host, memory| {
        let dst = memory.read_exact_mut(dst, dst_len)?;
        host.with_c_buffer(src, |src| {
            stub::blit_from_c(src, src_offset, dst, dst_offset, len)
        })
    })
}

#[ported(source = "src/internal/c_buffer/stub.c")]
pub(super) fn c_buffer_get(
    context: &mut ImportContext<'_, '_>,
    buf: u64,
    index: i32,
) -> AsyncHostResult<i32> {
    context.host
        .with_c_buffer(buf, |buf| stub::c_buffer_get(buf, index).map(i32::from))
}

#[ported(source = "src/internal/c_buffer/stub.c")]
pub(super) fn strlen(context: &mut ImportContext<'_, '_>, buf: u64) -> AsyncHostResult<i32> {
    context.host.with_c_buffer(buf, stub::strlen)
}

pub(super) fn length(context: &mut ImportContext<'_, '_>, buf: u64) -> AsyncHostResult<i32> {
    context
        .host
        .with_c_buffer(buf, |buf| i32::try_from(buf.len()).map_err(|_| AsyncHostError::Fault))
}

pub(super) fn free(context: &mut ImportContext<'_, '_>, ptr: u64) -> AsyncHostResult<()> {
    context.host.free_c_buffer(ptr)
}

#[ported(
    source = "src/internal/c_buffer/stub.c",
    original = "moonbitlang_async_make_c_buffer"
)]
pub(super) fn new(context: &mut ImportContext<'_, '_>, size: i32) -> AsyncHostResult<u64> {
    Ok(context.host.insert_c_buffer(stub::make_c_buffer(size)?))
}

fn checked_add_i32(lhs: i32, rhs: i32) -> AsyncHostResult<i32> {
    lhs.checked_add(rhs).ok_or(AsyncHostError::Fault)
}
}
