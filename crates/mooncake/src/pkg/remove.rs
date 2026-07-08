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

use anyhow::bail;
use std::{path::Path, sync::Arc};

use moonutil::{
    constants::MOON_MOD,
    manifest::{
        convert_module_to_mod_json, read_module_desc_file_in_dir, write_module_json_to_file,
    },
    moon_mod_patch::{MoonModPatch, patch_module_dsl_to_file},
    project::ProjectManifest,
};

use crate::{
    pkg::roots_for_selected_module,
    registry,
    resolver::{ResolveConfig, resolve_with_default_env_and_resolver},
};

/// Remove a dependency
#[derive(Debug, clap::Parser)]
pub struct RemoveSubcommand {
    /// The registry module name to remove
    #[clap(value_name = "MODULE")]
    pub package_path: String,
}

pub fn remove(
    module_dir: &Path,
    project_manifest: &ProjectManifest,
    username: &str,
    pkgname: &str,
) -> anyhow::Result<i32> {
    let mut m = read_module_desc_file_in_dir(module_dir)?;
    let dep_name = format!("{username}/{pkgname}");
    let removed = m.deps.shift_remove(&dep_name);
    if removed.is_none() {
        bail!(
            "the dependency `{}/{}` could not be found",
            username,
            pkgname,
        )
    }
    let m = Arc::new(m);
    let roots = roots_for_selected_module(module_dir, Arc::clone(&m), project_manifest)?;

    let resolve_cfg = ResolveConfig {
        registry: registry::default_registry(),
        inject_std: false, // no need to inject
    };
    resolve_with_default_env_and_resolver(&resolve_cfg, roots)?;

    if module_dir.join(MOON_MOD).exists() {
        patch_module_dsl_to_file(
            module_dir,
            MoonModPatch::RemoveImportItem { name: dep_name },
        )?;
    } else {
        let new_j = convert_module_to_mod_json(Arc::unwrap_or_clone(m));
        write_module_json_to_file(&new_j, module_dir)?;
    }
    Ok(0)
}
