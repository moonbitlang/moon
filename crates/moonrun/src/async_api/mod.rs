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

//! V8-facing `moonbitlang/async` import adapter.
//!
//! This layer owns the canonical wasm import list, decodes wasm ABI values from
//! callback arguments, acquires guest memory, sets return values, and reports
//! traps. Callback implementations are written against `ImportContext` so the
//! engine-neutral Host behavior stays separate from V8-specific memory access.
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
mod process;
mod provenance;
mod random;
mod registry;
mod runtime;
mod signal;
mod socket;
mod stdio;
mod thread_pool;
mod time;
mod tls;

use crate::v8_import::V8RunContext;

pub(crate) use registry::MOONBIT_ASYNC_MODULE;

/// # Safety
///
/// `context` must remain valid whenever a registered callback can be invoked.
pub(crate) unsafe fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    context: *const V8RunContext,
) {
    registry::register_imports(obj, scope, context);
}
