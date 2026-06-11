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

use std::ffi::OsStr;

use crate::async_host::{AsyncHostError, AsyncHostResult, GuestMemory, GuestRange};
use crate::async_sys::fs::dir;
use crate::async_sys::fs::stub;

use super::context::{
    AsyncContext, ImportArgs, callback_context, finish_bool, throw_import_error, with_memory_mut,
};

pub(super) fn get_tmp_path_len(
    _scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match get_tmp_path_len_impl() {
        Ok(len) => ret.set_int32(len),
        Err(error) => {
            context.host.record_error(error);
            ret.set_int32(-1);
        }
    }
}

pub(super) fn get_tmp_path(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match get_tmp_path_impl(scope, &args, context) {
        Ok(()) => ret.set_int32(0),
        Err(error) => {
            context.host.record_error(error);
            ret.set_int32(-1);
        }
    }
}

pub(super) fn close_fd(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context.host.close_fd(args.i32(0)?)
    })();
    match result {
        Ok(()) => ret.set_int32(0),
        Err(error) => {
            context.host.record_error(error);
            ret.set_int32(-1);
        }
    }
}

pub(super) fn dir_buffer_min_size(
    _scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    ret.set_int32(dir::buffer_min_size());
}

pub(super) fn dir_entry_length(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match with_dir_header(scope, &args, context, dir::entry_length) {
        Ok(value) => ret.set_int32(value),
        Err(error) => throw_import_error(scope, "dir_entry_length", error),
    }
}

pub(super) fn dir_entry_name_len(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match with_dir_header(scope, &args, context, dir::entry_name_len) {
        Ok(value) => ret.set_int32(value),
        Err(error) => throw_import_error(scope, "dir_entry_name_len", error),
    }
}

pub(super) fn dir_entry_name(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        let buf = args.i32(0)?;
        let offset = args.i32(1)?;
        with_memory_mut(scope, context, |memory| {
            let header = dir_entry_header(memory, buf, offset)?;
            dir::entry_name_len(header, 0)?;
            dir::entry_name_ptr(buf, offset)
        })
    })();
    match result {
        Ok(value) => ret.set_int32(value),
        Err(error) => throw_import_error(scope, "dir_entry_name", error),
    }
}

pub(super) fn dir_entry_is_dir(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match with_dir_header(scope, &args, context, dir::entry_is_dir) {
        Ok(value) => ret.set_int32(value),
        Err(error) => throw_import_error(scope, "dir_entry_is_dir", error),
    }
}

pub(super) fn dir_entry_is_hidden(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match with_dir_header(scope, &args, context, dir::entry_is_hidden) {
        Ok(value) => finish_bool(&mut ret, value),
        Err(error) => throw_import_error(scope, "dir_entry_is_hidden", error),
    }
}

pub(super) fn dir_entry_file_id(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match with_dir_header(scope, &args, context, dir::entry_file_id) {
        Ok(value) => ret.set(v8::BigInt::new_from_u64(scope, value).into()),
        Err(error) => throw_import_error(scope, "dir_entry_file_id", error),
    }
}

fn get_tmp_path_len_impl() -> AsyncHostResult<i32> {
    let bytes = encoded_tmp_path()?;
    i32::try_from(bytes.len()).map_err(|_| AsyncHostError::Fault)
}

fn get_tmp_path_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
) -> AsyncHostResult<()> {
    let mut args = ImportArgs::new(scope, args);
    let dst = args.i32(0)?;
    let len = args.i32(1)?;
    let bytes = encoded_tmp_path()?;
    let required_len = i32::try_from(bytes.len()).map_err(|_| AsyncHostError::Fault)?;

    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    if len < bytes.len() {
        return Err(AsyncHostError::Inval);
    }

    with_memory_mut(scope, context, |memory| {
        memory.write(GuestRange::new(dst, required_len)?, &bytes)
    })
}

fn encoded_tmp_path() -> AsyncHostResult<Vec<u8>> {
    // Native async returns a process-owned C buffer here. Wasm cannot receive
    // host pointers, so the V8 adapter exposes the same OS string value by
    // length-first copy-out in the representation async uses on this platform.
    let path = stub::get_tmp_path()?;
    Ok(encode_os_string_for_wasm(path.as_os_str()))
}

fn with_dir_header<T>(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    context: &AsyncContext,
    f: impl FnOnce(&[u8], i32) -> AsyncHostResult<T>,
) -> AsyncHostResult<T> {
    let mut args = ImportArgs::new(scope, args);
    let buf = args.i32(0)?;
    let offset = args.i32(1)?;
    with_memory_mut(scope, context, |memory| {
        let header = dir_entry_header(memory, buf, offset)?;
        f(header, 0)
    })
}

fn dir_entry_header(
    memory: &(impl GuestMemory + ?Sized),
    buf: i32,
    offset: i32,
) -> AsyncHostResult<&[u8]> {
    let header_ptr = buf.checked_add(offset).ok_or(AsyncHostError::Fault)?;
    memory.read(GuestRange::new(header_ptr, dir::HEADER_LEN as i32)?)
}

#[cfg(unix)]
fn encode_os_string_for_wasm(path: &OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;

    path.as_bytes().to_vec()
}

#[cfg(windows)]
fn encode_os_string_for_wasm(path: &OsStr) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;

    path.encode_wide()
        .flat_map(|code_unit| code_unit.to_le_bytes())
        .collect()
}

pub(super) fn errno_is_lock_violation(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let mut args = ImportArgs::new(scope, &args);
    match args.i32(0) {
        Ok(errno) => finish_bool(&mut ret, stub::errno_is_lock_violation(errno)),
        Err(error) => {
            context.host.record_error(error);
            finish_bool(&mut ret, false);
        }
    }
}

pub(super) fn try_lock_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        let fd = args.i32(0)?;
        let exclusive = args.i32(1)? != 0;
        context.host.try_lock_file(fd, exclusive)
    })();
    match result {
        Ok(()) => ret.set_int32(0),
        Err(error) => {
            context.host.record_error(error);
            ret.set_int32(-1);
        }
    }
}

pub(super) fn unlock_file(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        context.host.unlock_file(args.i32(0)?)
    })();
    match result {
        Ok(()) => ret.set_int32(0),
        Err(error) => {
            context.host.record_error(error);
            ret.set_int32(-1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn os_string_wasm_encoding_uses_unix_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let path = OsStr::from_bytes(b"/tmp/\xff");

        assert_eq!(encode_os_string_for_wasm(path), b"/tmp/\xff");
    }

    #[cfg(windows)]
    #[test]
    fn os_string_wasm_encoding_uses_utf16_code_units() {
        let path = OsStr::new("A\u{10000}");

        assert_eq!(
            encode_os_string_for_wasm(path),
            [0x41, 0x00, 0x00, 0xd8, 0x00, 0xdc]
        );
    }
}
