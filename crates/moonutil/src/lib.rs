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

#![warn(clippy::clone_on_ref_ptr)]

mod binaries;
pub mod cache;
pub mod child_process;
pub mod cli_support;
pub mod command_output;
pub mod compiler_flags;
pub mod cond_expr;
pub mod constants;
pub mod demangle;
pub mod dependency;
mod dirs;
pub mod error_code_docs;
pub mod features;
pub mod front_matter;
pub mod locks;
pub mod manifest;
mod module;
pub mod module_name;
mod moon_dir;
pub mod moon_pkg;
pub mod package;
pub mod path;
pub mod path_normalizer;
pub mod policy_transport;
pub mod project;
pub mod resolution;
pub mod run_handoff;
pub mod scripts;
pub mod shlex;
pub mod supported_targets;
pub mod target;
pub mod toolchain;
pub mod user_log;
pub mod version;
pub mod wasm_source_map;

pub use moon_dir::{MOON_HOME, MoonHomeLayout};
pub mod workspace;
