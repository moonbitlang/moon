// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! `moonbitlang/async` Wasm Adapter shared by the engine backends.
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

#[cfg(feature = "v8")]
use crate::v8::context::V8RunContext;

pub(crate) use registry::MOONBIT_ASYNC_MODULE;
#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
pub(crate) use registry::register_wasmtime_imports;

/// # Safety
///
/// `context` must remain valid whenever a registered callback can be invoked.
#[cfg(feature = "v8")]
pub(crate) unsafe fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    core_obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    context: *const V8RunContext,
) {
    registry::register_imports(obj, scope, context);
    registry::register_core_imports(core_obj, scope, context);
}
