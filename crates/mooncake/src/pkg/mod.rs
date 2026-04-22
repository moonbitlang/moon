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

use std::{path::Path, sync::Arc};

use anyhow::Context;
use moonutil::{
    common::{MOON_WORK, read_module_desc_file_in_dir},
    module::MoonMod,
    mooncakes::{
        ModuleSource,
        result::{ResolvedModule, ResolvedRootModules},
    },
    workspace::{canonical_workspace_module_dirs, read_workspace, read_workspace_file},
};

pub mod add;
pub mod install;
pub mod remove;
pub mod sync;
pub mod tree;
mod work;

pub use work::{init_workspace, sync_workspace, use_workspace};

pub(crate) fn roots_for_selected_module(
    project_root: &Path,
    module_dir: &Path,
    module: Arc<MoonMod>,
    project_manifest_path: Option<&Path>,
) -> anyhow::Result<ResolvedRootModules> {
    if let Some(project_manifest_path) = project_manifest_path {
        if project_manifest_path
            .file_name()
            .and_then(|name| name.to_str())
            == Some(MOON_WORK)
        {
            let workspace_root = project_manifest_path
                .parent()
                .context("workspace manifest path has no parent directory")?;
            let workspace = read_workspace_file(project_manifest_path)?;
            let mut roots = ResolvedRootModules::with_key();
            for member_dir in canonical_workspace_module_dirs(workspace_root, &workspace)? {
                let member = if member_dir == module_dir {
                    Arc::clone(&module)
                } else {
                    Arc::new(read_module_desc_file_in_dir(&member_dir)?)
                };
                let source = ModuleSource::from_local_module(&member, &member_dir);
                roots.insert(ResolvedModule::new(source, member));
            }
            return Ok(roots);
        }
    } else if let Some(workspace) = read_workspace(project_root)? {
        let mut roots = ResolvedRootModules::with_key();
        for member_dir in canonical_workspace_module_dirs(project_root, &workspace)? {
            let member = if member_dir == module_dir {
                Arc::clone(&module)
            } else {
                Arc::new(read_module_desc_file_in_dir(&member_dir)?)
            };
            let source = ModuleSource::from_local_module(&member, &member_dir);
            roots.insert(ResolvedModule::new(source, member));
        }
        return Ok(roots);
    }

    let source = ModuleSource::from_local_module(&module, module_dir);
    let (roots, _) = ResolvedModule::only_one_module(source, module);
    Ok(roots)
}
