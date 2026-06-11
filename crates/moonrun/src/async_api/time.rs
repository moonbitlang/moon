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
use crate::async_sys::internal::{event_loop::wasm_support, time::clock};

use super::context::{ImportArgs, callback_context, finish_errno};

pub(super) fn get_ms_since_epoch(
    scope: &mut v8::HandleScope,
    _args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let value = v8::BigInt::new_from_i64(scope, clock::get_ms_since_epoch());
    ret.set(value.into());
}

pub(super) fn sleep_ms(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    finish_errno(context, &mut ret, sleep_ms_impl(scope, &args));
}

fn sleep_ms_impl(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
) -> AsyncHostResult<()> {
    let mut args = ImportArgs::new(scope, args);
    let duration_ms = args.i32(0)?;
    wasm_support::sleep_ms(duration_ms);
    Ok(())
}
