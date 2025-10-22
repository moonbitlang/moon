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

//! Prebuild config (module-level) and logic.
//!
//! Currently, prebuild config runs unconditionally and before everything else,
//! but ultimately we might want to merge it into the main build graph. This is
//! a temporary solution.

use std::collections::HashMap;

use anyhow::Context;
use moonbuild::build_script::make_prebuild_input_from_module;
use moonutil::{
    build_script::{LinkConfig, RerunIfKind},
    mooncakes::{ModuleId, ModuleSource},
};
use tracing::instrument;

use crate::{ResolveOutput, model::PackageId};

/// The output of running prebuild config scripts
#[derive(Debug, Default)]
pub struct PrebuildOutput {
    pub module_outputs: HashMap<ModuleId, ModulePrebuildOutput>,
    pub package_configs: HashMap<PackageId, LinkConfig>,
}

/// A module's prebuild output
#[derive(Debug, Default)]
pub struct ModulePrebuildOutput {
    /// Conditions that might trigger rerun. Currently have no effect (always rerun).
    pub rerun_if: Vec<RerunIfKind>,
    /// Environment variables set by the prebuild script
    pub vars: HashMap<String, String>,
}

#[instrument(skip_all)]
pub fn run_prebuild_config(resolve_output: &ResolveOutput) -> anyhow::Result<PrebuildOutput> {
    let env_vars: HashMap<String, String> = std::env::vars().collect();
    let mut output = PrebuildOutput::default();

    // Run prebuild scripts
    for (m, ms) in resolve_output.module_rel.all_modules_and_id() {
        run_prebuild_for_module(m, ms, resolve_output, &env_vars, &mut output)?;
    }

    Ok(output)
}

fn run_prebuild_for_module(
    m: ModuleId,
    ms: &ModuleSource,
    resolve_output: &ResolveOutput,
    env_vars: &HashMap<String, String>,
    ret: &mut PrebuildOutput,
) -> anyhow::Result<()> {
    let m_info = &**resolve_output.module_rel.module_info(m);
    let m_dir = resolve_output.module_dirs.get(m).expect("module not found");
    let Some(prebuild) = &m_info.__moonbit_unstable_prebuild else {
        return Ok(());
    };

    // Run the prebuild script
    let input = make_prebuild_input_from_module(m_dir, env_vars);
    let output = moonbuild::build_script::run_build_script_for_module(ms, m_dir, input, prebuild)
        .context(format!(
        "Failed to run prebuild script for module {}",
        m_info.name.as_str()
    ))?;

    // Insert module-level configs
    let module_output = ModulePrebuildOutput {
        rerun_if: output.rerun_if,
        vars: output.vars,
    };
    ret.module_outputs.insert(m, module_output);

    // Insert package-level configs
    let module_name = &m_info.name;
    let packages = resolve_output
        .pkg_dirs
        .packages_for_module(m)
        .expect("module has no packages");
    for cfg in output.link_configs {
        // Find the package ID from its name
        let stripped_pkg_name = cfg
            .package
            .strip_prefix(module_name.as_str())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Link config package name {} does not start with module name {}, cannot apply config to an external package",
                    cfg.package,
                    module_name
                )
            })?
            .trim_start_matches('/');
        let pkg_id = *packages.get(stripped_pkg_name).ok_or_else(|| {
            anyhow::anyhow!(
                "Link config package name {} does not match any package in module {}",
                cfg.package,
                module_name
            )
        })?;
        ret.package_configs.insert(pkg_id, cfg);
    }

    Ok(())
}
