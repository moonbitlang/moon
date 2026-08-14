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

//! Embeddable execution support for MoonBit Wasm programs.
//!
//! The interface is experimental. It currently separates guest termination
//! from host-process termination, while retaining Moonrun's existing V8 and
//! process-scoped host integrations.

mod async_api;
mod async_host;
mod async_policy;
mod async_sys;
mod backtrace_api;
mod demangle_js_template;
mod fs_api_temp;
mod host;
mod host_fs;
mod host_imports;
mod memory_sanitizer_api;
mod run_termination;
mod runtime;
mod sys_api;
mod util;
mod v8_backend;
mod v8_builder;
mod v8_import;
mod wasi_api;

pub use runtime::{RunOptions, RunOutcome, Runtime, RuntimeConfig};
