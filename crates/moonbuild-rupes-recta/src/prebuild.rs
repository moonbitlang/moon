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

//! Module-level prebuild configuration consumed by build planning.
//!
//! The command layer owns script compilation, execution, and output validation.

use std::collections::HashMap;

use moonutil::{
    build_script::{LinkConfig, RerunIfKind},
    resolution::ModuleId,
};

use crate::model::PackageId;

/// The output of running prebuild config scripts
#[derive(Debug, Default)]
pub struct PrebuildOutput {
    pub module_outputs: HashMap<ModuleId, ModulePrebuildOutput>,
    pub package_configs: HashMap<PackageId, LinkConfig>,
}

/// A module's prebuild output
#[derive(Debug)]
pub struct ModulePrebuildOutput {
    /// Conditions that might trigger rerun. Currently have no effect (always rerun).
    pub rerun_if: Vec<RerunIfKind>,
    /// Environment variables set by the prebuild script
    pub vars: HashMap<String, String>,
}
