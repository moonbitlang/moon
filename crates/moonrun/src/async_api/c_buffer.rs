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

use crate::async_host::{AsyncHostError, AsyncHostResult, checked_mut_range, checked_range};
use crate::async_sys::internal::c_buffer::stub;

use super::context::{
    AsyncContext, ImportArgs, callback_context, finish_bool, throw_import_error, with_memory_mut,
};

pub(super) fn blit_to_c(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    if let Err(error) = blit_to_c_impl(scope, &args, context) {
        throw_import_error(scope, "blit_to_c", error);
        return;
    }
    ret.set_undefined();
}

pub(super) fn blit_from_c(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    if let Err(error) = blit_from_c_impl(scope, &args, context) {
        throw_import_error(scope, "blit_from_c", error);
        return;
    }
    ret.set_undefined();
}

pub(super) fn c_buffer_get(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match c_buffer_get_impl(scope, &args, context) {
        Ok(byte) => ret.set_int32(i32::from(byte)),
        Err(error) => throw_import_error(scope, "c_buffer_get", error),
    }
}

pub(super) fn strlen(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match strlen_impl(scope, &args, context) {
        Ok(len) => ret.set_int32(len),
        Err(error) => throw_import_error(scope, "strlen", error),
    }
}

pub(super) fn null_pointer(
    _scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    ret.set_int32(stub::null_pointer());
}

pub(super) fn pointer_is_null(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let mut args = ImportArgs::new(scope, &args);
    match args.i32(0) {
        Ok(ptr) => finish_bool(&mut ret, stub::pointer_is_null(ptr)),
        Err(error) => throw_import_error(scope, "pointer_is_null", error),
    }
}

fn blit_to_c_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<()> {
    let mut args = ImportArgs::new(scope, args);
    let dst = args.i32(0)?;
    let src = args.i32(1)?;
    let offset = args.i32(2)?;
    let len = args.i32(3)?;
    let src_len = checked_add_i32(offset, len)?;
    with_memory_mut(scope, context, |memory| {
        let src = checked_range(memory, src, src_len)?.to_vec();
        let dst = checked_mut_range(memory, dst, len)?;
        stub::blit_to_c(dst, &src, offset, len)
    })
}

fn blit_from_c_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<()> {
    let mut args = ImportArgs::new(scope, args);
    let src = args.i32(0)?;
    let dst = args.i32(1)?;
    let offset = args.i32(2)?;
    let len = args.i32(3)?;
    let dst_len = checked_add_i32(offset, len)?;
    with_memory_mut(scope, context, |memory| {
        let src = checked_range(memory, src, len)?.to_vec();
        let dst = checked_mut_range(memory, dst, dst_len)?;
        stub::blit_from_c(&src, dst, offset, len)
    })
}

fn c_buffer_get_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<u8> {
    let mut args = ImportArgs::new(scope, args);
    let buf = args.i32(0)?;
    let index = args.i32(1)?;
    let len = checked_add_i32(index, 1)?;
    with_memory_mut(scope, context, |memory| {
        let buf = checked_range(memory, buf, len)?;
        stub::c_buffer_get(buf, index)
    })
}

fn strlen_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<i32> {
    let mut args = ImportArgs::new(scope, args);
    let buf = args.i32(0)?;
    let offset = usize::try_from(buf).map_err(|_| AsyncHostError::Fault)?;
    with_memory_mut(scope, context, |memory| {
        let buf = memory.get(offset..).ok_or(AsyncHostError::Fault)?;
        stub::strlen(buf)
    })
}

fn checked_add_i32(lhs: i32, rhs: i32) -> AsyncHostResult<i32> {
    lhs.checked_add(rhs).ok_or(AsyncHostError::Fault)
}
