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

//! Runtime-facing `moonbitlang/async` import adapter.
//!
//! This layer owns the canonical wasm import list, decodes wasm ABI values from
//! callback arguments, acquires guest memory, sets return values, and reports
//! traps. Callback implementations are written against `ImportContext` so the
//! host behavior stays separate from runtime-specific memory access.
//! Ported native-stub behavior belongs in `async_sys`; shared runtime state
//! belongs in `async_host`.

mod c_buffer;
mod context;
mod env_util;
mod event_bus;
mod event_loop;
mod fd_util;
mod fs;
mod io;
mod os_error;
mod os_string;
mod provenance;
mod registry;
mod runtime;
mod socket;
mod thread_pool;
mod time;

use std::any::Any;

use crate::async_host::AsyncHost;

pub(crate) use registry::MOONBIT_ASYNC_MODULE;

pub(crate) fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    dtors: &mut Vec<Box<dyn Any>>,
) {
    let context = Box::new(context::AsyncContext::new(scope, obj, AsyncHost::default()));
    let context_ptr = &*context as *const context::AsyncContext;
    dtors.push(context);

    registry::register_imports(obj, scope, context_ptr);
}
