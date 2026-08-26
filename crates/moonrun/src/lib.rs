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
mod async_sys;
mod engine;
mod filesystem;
mod guest_memory;
mod network;
mod policy;
mod process;
mod resource;
mod run_termination;
mod runtime;
mod source_map;
mod sqlite;
mod temp_dir;
mod util;
mod v8;

pub use engine::{Engine, EngineConfig, Module, RunOptions, RunOutcome};
pub use runtime::WorkingDirectory;

#[doc(hidden)]
pub fn consume_inherited_policy_copy(token: std::ffi::OsString) -> anyhow::Result<Vec<u8>> {
    policy::consume_inherited_copy(token)
}
