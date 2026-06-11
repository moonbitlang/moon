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

use crate::async_host::{AsyncHostError, AsyncHostResult, GuestMemory, GuestRange};

use super::context::{
    AsyncContext, ImportArgs, callback_context, throw_import_error, with_memory_mut,
};

const FILE_TIME_RECORD_LEN: i32 = 48;
const ATIME_SEC_OFFSET: i32 = 0;
const ATIME_NSEC_OFFSET: i32 = 8;
const MTIME_SEC_OFFSET: i32 = 16;
const MTIME_NSEC_OFFSET: i32 = 24;
const CTIME_SEC_OFFSET: i32 = 32;
const CTIME_NSEC_OFFSET: i32 = 40;

pub(super) fn sizeof_file_time(
    _scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    ret.set_int32(FILE_TIME_RECORD_LEN);
}

pub(super) fn get_atime_sec(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    file_time_i64(scope, &args, ATIME_SEC_OFFSET, "get_atime_sec", &mut ret);
}

pub(super) fn get_atime_nsec(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    file_time_i32(scope, &args, ATIME_NSEC_OFFSET, "get_atime_nsec", &mut ret);
}

pub(super) fn get_mtime_sec(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    file_time_i64(scope, &args, MTIME_SEC_OFFSET, "get_mtime_sec", &mut ret);
}

pub(super) fn get_mtime_nsec(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    file_time_i32(scope, &args, MTIME_NSEC_OFFSET, "get_mtime_nsec", &mut ret);
}

pub(super) fn get_ctime_sec(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    file_time_i64(scope, &args, CTIME_SEC_OFFSET, "get_ctime_sec", &mut ret);
}

pub(super) fn get_ctime_nsec(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    file_time_i32(scope, &args, CTIME_NSEC_OFFSET, "get_ctime_nsec", &mut ret);
}

pub(super) fn pipe(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        let dst = args.i32(0)?;
        let len = args.i32(1)?;
        if len < 2 {
            return Err(AsyncHostError::Fault);
        }
        let fds = context.host.pipe()?;
        with_memory_mut(scope, context, |memory| {
            memory.write_i32_le(dst, fds[0])?;
            memory.write_i32_le(dst.checked_add(4).ok_or(AsyncHostError::Fault)?, fds[1])
        })
    })();
    match result {
        Ok(()) => ret.set_int32(0),
        Err(error) => {
            context.host.record_error(error);
            ret.set_int32(-1);
        }
    }
}

fn file_time_i64(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    field_offset: i32,
    name: &str,
    ret: &mut v8::ReturnValue,
) {
    let context = callback_context(args);
    match read_field(scope, args, context, field_offset, 8)
        .map(|bytes| i64::from_le_bytes(bytes.as_slice().try_into().unwrap()))
    {
        Ok(value) => ret.set(v8::BigInt::new_from_i64(scope, value).into()),
        Err(error) => throw_import_error(scope, name, error),
    }
}

fn file_time_i32(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    field_offset: i32,
    name: &str,
    ret: &mut v8::ReturnValue,
) {
    let context = callback_context(args);
    match read_field(scope, args, context, field_offset, 4)
        .map(|bytes| i32::from_le_bytes(bytes.as_slice().try_into().unwrap()))
    {
        Ok(value) => ret.set_int32(value),
        Err(error) => throw_import_error(scope, name, error),
    }
}

fn read_field(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
    field_offset: i32,
    len: i32,
) -> AsyncHostResult<Vec<u8>> {
    let mut args = ImportArgs::new(scope, args);
    let ptr = args.i32(0)?;
    let offset = ptr.checked_add(field_offset).ok_or(AsyncHostError::Fault)?;
    with_memory_mut(scope, context, |memory| {
        Ok(memory.read(GuestRange::new(offset, len)?)?.to_vec())
    })
}
