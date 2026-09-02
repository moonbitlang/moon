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
//! The interface is experimental. It separates guest termination from
//! host-process termination while retaining Moonrun's existing process-scoped
//! host integrations.

mod async_api;
mod async_host;
mod async_sys;
mod engine;
mod filesystem;
mod guest_memory;
mod memory_sanitizer;
mod network;
mod policy;
mod process;
mod resource;
mod run_termination;
mod runtime;
mod source_map;
mod sqlite;
mod util;
mod v8;
mod wasi;
mod wasm_diagnostic;

pub use engine::{Engine, EngineConfig, Module, RunOptions, RunOutcome};
pub use runtime::WorkingDirectory;
