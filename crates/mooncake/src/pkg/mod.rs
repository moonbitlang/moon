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
    common::read_module_desc_file_in_dir,
    dirs::ProjectManifest,
    module::MoonMod,
    mooncakes::{
        ModuleSource,
        result::{ResolvedModule, ResolvedRootModules},
    },
    workspace::{canonical_workspace_module_dirs, read_workspace_file},
};

pub mod add;
pub mod install;
pub mod remove;
pub mod sync;
pub mod tree;
mod work;

pub use work::{init_workspace, sync_workspace, use_workspace};

pub(crate) fn roots_for_selected_module(
    module_dir: &Path,
    module: Arc<MoonMod>,
    project_manifest: &ProjectManifest,
) -> anyhow::Result<ResolvedRootModules> {
    if let ProjectManifest::Workspace(project_manifest) = project_manifest {
        let workspace_root = project_manifest
            .parent()
            .context("workspace manifest path has no parent directory")?;
        let workspace = read_workspace_file(project_manifest)?;
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

    let source = ModuleSource::from_local_module(&module, module_dir);
    let (roots, _) = ResolvedModule::only_one_module(source, module);
    Ok(roots)
}
