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

//! Synchronous Wasm adapter for the SQLite Host interface.
//!
//! The `sqlite3_*` imports keep SQLite-shaped operations while adapting native
//! pointers to a portable wasm ABI. Guest-memory pointers are unsigned wasm
//! `i32` offsets, while SQLite-owned objects and the reserved VFS parameter are
//! opaque `u64` Handles. The guest obtains their shared null value at runtime
//! through `sqlite3_null_handle`.
//!
//! UTF-16 inputs and outputs use the little-endian code units stored by wasm.
//! `sqlite3_prepare16_v2` receives a backing String plus a code-unit offset and
//! length, and returns `pzTail` as an absolute code-unit offset in that same
//! String. This preserves multiple-statement and `StringView` semantics without
//! exposing a runtime-specific address. UTF-16 and blob bindings are copied by
//! SQLite before the Guest Memory borrow ends. Error messages and variable-size
//! columns use length-and-copy pairs instead of exposing borrowed native
//! pointers. Column names follow the same length-and-copy convention. SQLite
//! behavior and policy belong to the parent `sqlite` module; this adapter only
//! lowers engine values and Guest Memory.
//! Callback-bearing extension APIs, varargs, process-global configuration,
//! custom VFSes, and file-backed databases are outside the MVP.

mod bind;
mod column;
mod connection;
mod context;
mod registry;
mod registry_macros;
mod statement;

#[cfg(feature = "v8")]
use crate::v8::context::V8RunContext;

pub(crate) use registry::MOONBIT_SQLITE_MODULE;
#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
pub(crate) use registry::register_wasmtime_imports;

/// # Safety
///
/// `context` must remain valid whenever a registered callback can be invoked.
#[cfg(feature = "v8")]
pub(crate) unsafe fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    context: *const V8RunContext,
) {
    // SAFETY: the caller retains the per-run V8 context throughout guest
    // execution and does not re-enter V8 after dropping it.
    unsafe { registry::register_imports(obj, scope, context) };
}
