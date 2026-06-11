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

use crate::async_host::AsyncHostResult;
use crate::async_sys::os_error::stub;

use super::context::{ImportArgs, callback_context, finish_bool};

pub(super) fn get_errno(
    _scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    ret.set_int32(stub::get_errno(&context.host));
}

fn is_errno_predicate(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    predicate: impl FnOnce(i32) -> bool,
) -> AsyncHostResult<bool> {
    let mut args = ImportArgs::new(scope, args);
    let errno = args.i32(0)?;
    Ok(predicate(errno))
}

macro_rules! errno_predicate {
    ($callback:ident, $function:ident) => {
        pub(super) fn $callback(
            scope: &mut v8::HandleScope,
            args: v8::FunctionCallbackArguments,
            mut ret: v8::ReturnValue,
        ) {
            let context = callback_context(&args);
            match is_errno_predicate(scope, &args, stub::$function) {
                Ok(value) => finish_bool(&mut ret, value),
                Err(error) => {
                    context.host.record_error(error);
                    finish_bool(&mut ret, false);
                }
            }
        }
    };
}

errno_predicate!(is_nonblocking_io_error, is_nonblocking_io_error);
errno_predicate!(is_eintr, is_eintr);
errno_predicate!(is_enoent, is_enoent);
errno_predicate!(is_eexist, is_eexist);
errno_predicate!(is_eacces, is_eacces);
errno_predicate!(is_econnrefused, is_econnrefused);
errno_predicate!(is_error_notify_enum_dir, is_error_notify_enum_dir);

pub(super) fn get_enotdir(
    _scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    ret.set_int32(stub::get_enotdir());
}
