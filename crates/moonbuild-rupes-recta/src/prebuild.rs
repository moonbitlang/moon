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
