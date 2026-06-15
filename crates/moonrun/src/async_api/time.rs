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

#[cfg(unix)]
use crate::async_host::AsyncHostError;
use crate::async_host::AsyncHostResult;
use crate::async_sys::internal::time::clock;

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
    let result = (|| {
        let mut args = ImportArgs::new(scope, &args);
        sleep_ms_impl(args.i32(0)?)
    })();
    finish_errno(context, &mut ret, result);
}

fn sleep_ms_impl(duration_ms: i32) -> AsyncHostResult<()> {
    if duration_ms <= 0 {
        return Ok(());
    }
    sleep_ms_sys(duration_ms)
}

#[cfg(unix)]
fn sleep_ms_sys(duration_ms: i32) -> AsyncHostResult<()> {
    if unsafe { libc::poll(std::ptr::null_mut(), 0, duration_ms) } < 0 {
        return Err(AsyncHostError::Native(errno()));
    }
    Ok(())
}

#[cfg(unix)]
fn errno() -> i32 {
    #[cfg(target_os = "linux")]
    {
        unsafe { *libc::__errno_location() }
    }
    #[cfg(target_os = "macos")]
    {
        unsafe { *libc::__error() }
    }
}

#[cfg(windows)]
fn sleep_ms_sys(duration_ms: i32) -> AsyncHostResult<()> {
    unsafe {
        windows_sys::Win32::System::Threading::Sleep(duration_ms as u32);
    }
    Ok(())
}
