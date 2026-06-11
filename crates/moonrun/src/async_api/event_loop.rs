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

use crate::async_sys::internal::event_loop::thread_pool;

use super::context::{ImportArgs, callback_context, finish_bool};

pub(super) fn get_platform(
    _scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    ret.set_int32(thread_pool::get_platform().as_i32());
}

pub(super) fn errno_is_cancelled(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let mut args = ImportArgs::new(scope, &args);
    match args.i32(0) {
        Ok(errno) => finish_bool(&mut ret, thread_pool::errno_is_cancelled(errno)),
        Err(error) => {
            context.host.record_error(error);
            finish_bool(&mut ret, false);
        }
    }
}
