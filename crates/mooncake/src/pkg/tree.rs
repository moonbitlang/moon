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

use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use moonutil::manifest::read_module_desc_file_in_dir;
use moonutil::project::ProjectManifest;
use moonutil::resolution::{ModuleId, ModuleName, ResolvedEnv};
use moonutil::user_log::UserLog;

use crate::pkg::roots_for_selected_module;
use crate::registry;
use crate::resolver::{ResolveConfig, resolve_with_default_env_and_resolver};

/// Display the dependency tree
#[derive(Debug, clap::Parser)]
pub struct TreeSubcommand {
    /// Output one complete JSON result to stdout
    #[clap(long)]
    pub json: bool,

    /// Show the package-level dependency graph instead of the module-level tree
    #[clap(long)]
    pub package: bool,
}

/// The resolved dependency graph of the selected module, together with the
/// module the tree is rooted at.
pub struct ResolvedTree {
    pub env: ResolvedEnv,
    pub root: ModuleId,
}

pub fn tree(
    module_dir: &Path,
    project_manifest: &ProjectManifest,
    user_log: &UserLog,
) -> anyhow::Result<ResolvedTree> {
    let module = Arc::new(read_module_desc_file_in_dir(module_dir)?);
    let roots = roots_for_selected_module(module_dir, Arc::clone(&module), project_manifest)?;
    let registry = registry::default_registry();
    let resolve_cfg = ResolveConfig {
        registry: &registry,
        inject_std: false,
    };
    let resolved = resolve_with_default_env_and_resolver(&resolve_cfg, roots, user_log)?;

    let module_name: ModuleName = module.name.as_str().into();
    let selected_root = resolved
        .input_module_ids()
        .iter()
        .copied()
        .find(|id| resolved.module_source(*id).name() == &module_name)
        .or_else(|| resolved.input_module_ids().first().copied())
        .context("resolved dependency graph has no root modules")?;

    Ok(ResolvedTree {
        env: resolved,
        root: selected_root,
    })
}
