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

pub(super) fn exit(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    _ret: v8::ReturnValue,
) {
    let code = args.get(0).int32_value(scope).unwrap_or(1);
    std::process::exit(code);
}

pub(super) fn wait_for_event(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let timeout_ms = args.get(0).int32_value(scope).unwrap_or(-1);
    match wait_for_event_impl(timeout_ms) {
        Ok(()) => {
            super::set_last_errno(0);
            ret.set_int32(0);
        }
        Err(errno) => {
            super::set_last_errno(errno);
            ret.set_int32(-1);
        }
    }
}

#[cfg(unix)]
fn wait_for_event_impl(timeout_ms: i32) -> Result<(), i32> {
    let timeout = if timeout_ms < 0 { -1 } else { timeout_ms };
    if unsafe { libc::poll(std::ptr::null_mut(), 0, timeout) } < 0 {
        return Err(last_errno());
    }
    Ok(())
}

#[cfg(unix)]
fn last_errno() -> i32 {
    #[cfg(target_os = "linux")]
    {
        unsafe { *libc::__errno_location() }
    }
    #[cfg(target_os = "macos")]
    {
        unsafe { *libc::__error() }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(1)
    }
}

#[cfg(windows)]
fn wait_for_event_impl(timeout_ms: i32) -> Result<(), i32> {
    let timeout = if timeout_ms < 0 {
        windows_sys::Win32::System::Threading::INFINITE
    } else {
        timeout_ms as u32
    };
    unsafe {
        windows_sys::Win32::System::Threading::Sleep(timeout);
    }
    Ok(())
}
