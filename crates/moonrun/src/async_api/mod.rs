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

use std::cell::Cell;

mod event_loop;
mod os_error;
mod registry;
mod runtime;
mod thread_pool;
mod time;
mod unsupported;

pub(crate) use registry::MOONBIT_V0_MODULE;

thread_local! {
    static LAST_ERRNO: Cell<i32> = const { Cell::new(0) };
}

pub(crate) fn init_env<'s>(obj: v8::Local<'s, v8::Object>, scope: &mut v8::HandleScope<'s>) {
    registry::register_imports(obj, scope);
}

fn set_last_errno(errno: i32) {
    LAST_ERRNO.with(|last_errno| last_errno.set(errno));
}

fn last_errno() -> i32 {
    LAST_ERRNO.with(Cell::get)
}
