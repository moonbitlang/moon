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
pub(super) fn is_null(_context: &mut ImportContext, ptr: u64) -> i32 {
    i32::from(ptr == crate::async_host::INVALID_HOST_HANDLE)
}

#[ported(source = "src/internal/c_buffer/stub.c")]
pub(super) fn blit_to_c(
    context: &mut ImportContext,
    dst: u64,
    src: i32,
    offset: i32,
    len: i32,
) -> AsyncHostResult<()> {
    let src_len = checked_add_i32(offset, len)?;
    let src = context.with_memory_mut(|memory| Ok(memory.read_exact(src, src_len)?.to_vec()))?;
    context
        .host
        .with_c_buffer_mut(dst, |dst| stub::blit_to_c(dst, &src, offset, len))
}

#[ported(source = "src/internal/c_buffer/stub.c")]
pub(super) fn blit_from_c(
    context: &mut ImportContext,
    src: u64,
    dst: i32,
    offset: i32,
    len: i32,
) -> AsyncHostResult<()> {
    let dst_len = checked_add_i32(offset, len)?;
    let src = context.host.with_c_buffer(src, |src| {
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        Ok(src.get(..len).ok_or(AsyncHostError::Fault)?.to_vec())
    })?;
    context.with_memory_mut(|memory| {
        let dst = memory.read_exact_mut(dst, dst_len)?;
        stub::blit_from_c(&src, dst, offset, len)
    })
}

#[ported(source = "src/internal/c_buffer/stub.c")]
pub(super) fn c_buffer_get(
    context: &mut ImportContext,
    buf: u64,
    index: i32,
) -> AsyncHostResult<i32> {
    context
        .host
        .with_c_buffer(buf, |buf| stub::c_buffer_get(buf, index).map(i32::from))
}

#[ported(source = "src/internal/c_buffer/stub.c")]
pub(super) fn strlen(context: &mut ImportContext, buf: u64) -> AsyncHostResult<i32> {
    context.host.with_c_buffer(buf, stub::strlen)
}

pub(super) fn free(context: &mut ImportContext, ptr: u64) -> AsyncHostResult<()> {
    context.host.free_c_buffer(ptr)
}

fn checked_add_i32(lhs: i32, rhs: i32) -> AsyncHostResult<i32> {
    lhs.checked_add(rhs).ok_or(AsyncHostError::Fault)
}
}
