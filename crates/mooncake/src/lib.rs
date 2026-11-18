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

pub mod cli;
pub(crate) mod dependency_source;
pub mod pkg;
pub mod registry;
pub(crate) mod resolver;
pub(crate) mod zip_util;

// Shared inputs for package-management clients. Keep the existing type identity
// and the common manifest, project, and toolchain implementations in moonutil.
pub use moonutil::{
    MOON_HOME,
    constants::{MOON_MOD, MOON_MOD_JSON},
    manifest::{MoonMod, read_module_desc_file_in_dir, read_module_from_json},
    project::{PackageDirs, ProjectManifest},
    toolchain::{BINARIES, toolchain_root},
    user_log::UserLog,
};
