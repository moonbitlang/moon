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

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use moonutil::manifest::read_module_desc_file_in_dir;
use moonutil::project::ProjectManifest;
use moonutil::resolution::{ModuleDependencyGraph, ModuleId, ModuleName};
use moonutil::user_log::UserLog;

use crate::pkg::roots_for_selected_module;
use crate::registry;
use crate::resolver::{ResolveConfig, resolve_modules};

/// The resolved dependency graph of the selected module, together with the
/// module the tree is rooted at.
#[derive(Debug)]
pub struct ResolvedTree {
    pub env: ModuleDependencyGraph,
    pub root: ModuleId,
    pub workspace_members: HashSet<ModuleId>,
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
    let resolved = resolve_modules(&resolve_cfg, roots, user_log)?;

    let module_name: ModuleName = module.name.as_str().into();
    let selected_root = resolved
        .input_module_ids()
        .iter()
        .copied()
        .find(|id| resolved.module_source(*id).name() == &module_name)
        .or_else(|| resolved.input_module_ids().first().copied())
        .context("resolved dependency graph has no root modules")?;

    let workspace_members = if matches!(project_manifest, ProjectManifest::Workspace(_)) {
        resolved.input_module_ids().iter().copied().collect()
    } else {
        HashSet::new()
    };

    Ok(ResolvedTree {
        env: resolved,
        root: selected_root,
        workspace_members,
    })
}
