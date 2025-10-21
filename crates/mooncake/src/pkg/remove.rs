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
    common::{read_module_desc_file_in_dir, write_module_json_to_file},
    module::convert_module_to_mod_json,
    mooncakes::{ModuleSource, RegistryConfig},
};

use crate::resolver::{resolve_single_root_with_defaults, ResolveConfig};

/// Remove a dependency
#[derive(Debug, clap::Parser)]
pub struct RemoveSubcommand {
    /// The package path to remove
    pub package_path: String,
}

pub fn remove(
    source_dir: &Path,
    target_dir: &Path,
    username: &str,
    pkgname: &str,
    _registry_config: &RegistryConfig,
) -> anyhow::Result<i32> {
    let _ = target_dir;
    let mut m = read_module_desc_file_in_dir(source_dir)?;
    let removed = m.deps.shift_remove(&format!("{username}/{pkgname}"));
    if removed.is_none() {
        bail!(
            "the dependency `{}/{}` could not be found",
            username,
            pkgname,
        )
    }
    let m = Arc::new(m);
    let ms = ModuleSource::from_local_module(&m, source_dir).expect("Malformed module manifest");

    let registry = crate::registry::RegistryList::with_default_registry();
    let resolve_cfg = ResolveConfig {
        registries: registry,
        inject_std: false, // no need to inject
    };
    let res = resolve_single_root_with_defaults(&resolve_cfg, ms, Arc::clone(&m))?;

    drop(res);

    let new_j = convert_module_to_mod_json(Arc::into_inner(m).unwrap());
    write_module_json_to_file(&new_j, source_dir)?;
    Ok(0)
}
