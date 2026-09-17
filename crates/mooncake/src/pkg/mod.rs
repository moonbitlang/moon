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

use std::{path::Path, sync::Arc};

use moonutil::{
    manifest::{MoonMod, read_module_desc_file_in_dir},
    project::ProjectManifest,
    resolution::{ModuleSource, ResolvedModule, ResolvedRootModules},
};

pub mod add;
pub mod install;
pub mod legacy_postadd;
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
    if let ProjectManifest::Workspace(workspace) = project_manifest {
        let mut roots = ResolvedRootModules::with_key();
        for member_dir in workspace.members() {
            let member = if member_dir == module_dir {
                Arc::clone(&module)
            } else {
                Arc::new(read_module_desc_file_in_dir(member_dir)?)
            };
            let source = ModuleSource::from_local_module(&member, member_dir)?;
            roots.insert(ResolvedModule::new(source, member));
        }
        return Ok(roots);
    }

    let source = ModuleSource::from_local_module(&module, module_dir)?;
    let (roots, _) = ResolvedModule::only_one_module(source, module);
    Ok(roots)
}
