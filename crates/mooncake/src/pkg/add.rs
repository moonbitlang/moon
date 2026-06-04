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
use colored::Colorize;
use indexmap::IndexMap;
use moonutil::common::{
    MOON_MOD, MOONBITLANG_CORE, read_module_desc_file_in_dir, write_module_json_to_file,
};
use moonutil::dependency::{BinaryDependencyInfo, SourceDependencyInfo};
use moonutil::module::convert_module_to_mod_json;
use moonutil::moon_mod_patch::{MoonModPatch, patch_module_dsl_to_file};
use moonutil::mooncakes::ModuleName;
use semver::Version;
use std::path::Path;
use std::sync::Arc;

use crate::pkg::{install::install_impl, roots_for_selected_module, sync::SyncOutputOptions};
use crate::registry::{self, Registry};

/// Add a dependency
#[derive(Debug, clap::Parser)]
pub struct AddSubcommand {
    /// The registry module name to add
    #[clap(value_name = "MODULE")]
    pub package_path: String,

    /// Whether to add the dependency as a binary
    #[clap(long)]
    pub bin: bool,

    /// Upgrade an existing dependency
    #[clap(short = 'u', long)]
    pub upgrade: bool,

    /// Do not update the registry index before adding the dependency
    #[clap(long)]
    pub no_update: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn add_latest(
    project_root: &Path,
    module_dir: &Path,
    project_manifest_path: Option<&Path>,
    mooncakes_dir: &Path,
    pkg_name: &ModuleName,
    bin: bool,
    quiet: bool,
    index_updated: bool,
    upgrade: bool,
) -> anyhow::Result<i32> {
    let pkg_name_str = pkg_name.to_string();
    if pkg_name_str == MOONBITLANG_CORE {
        eprintln!(
            "{}: no need to add `{}` as dependency",
            "Warning".yellow().bold(),
            MOONBITLANG_CORE
        );
        std::process::exit(0);
    }

    let registry = registry::OnlineRegistry::mooncakes_io();
    let latest_version = registry
        .get_latest_version(pkg_name)
        .ok_or_else(|| {
            if index_updated {
                anyhow::anyhow!(
                    "Could not find the latest published version of `{}` in the registry",
                    pkg_name_str
                )
            } else {
                anyhow::anyhow!(
                    "Could not find the latest published version of `{}` in the registry. Please consider running `moon update` to update the index.",
                    pkg_name_str
                )
            }
        })?
        .version
        .clone()
        .unwrap();
    add(
        project_root,
        module_dir,
        project_manifest_path,
        mooncakes_dir,
        pkg_name,
        bin,
        &latest_version,
        quiet,
        upgrade,
    )
}

#[test]
fn test_module_name() {
    let core_name = MOONBITLANG_CORE.parse::<ModuleName>().unwrap();
    assert_eq!(MOONBITLANG_CORE, core_name.to_string());
}

#[allow(clippy::too_many_arguments)]
pub fn add(
    project_root: &Path,
    module_dir: &Path,
    project_manifest_path: Option<&Path>,
    mooncakes_dir: &Path,
    pkg_name: &ModuleName,
    bin: bool,
    version: &Version,
    quiet: bool,
    upgrade: bool,
) -> anyhow::Result<i32> {
    let mut m = read_module_desc_file_in_dir(module_dir)?;

    let pkg_name_str = pkg_name.to_string();
    if pkg_name_str == MOONBITLANG_CORE {
        eprintln!(
            "{}: no need to add `{}` as dependency",
            "Warning".yellow().bold(),
            MOONBITLANG_CORE
        );
        std::process::exit(0);
    }

    if upgrade {
        let Some(dep) = m.deps.get_mut(&pkg_name_str) else {
            bail!(
                "the dependency `{pkg_name_str}` could not be found; use `moon add {pkg_name_str}` to add it"
            );
        };

        if dep.path().is_some() || dep.git().is_some() {
            bail!("dependency `{pkg_name_str}` is not a registry dependency");
        }

        if dep.version() == Some(version) {
            eprintln!(
                "{}: dependency `{pkg_name_str}` is already at version {version}",
                "Warning".yellow().bold(),
            );
            return Ok(0);
        }

        dep.set_version(Some(version.clone()));
    } else if bin {
        let bin_deps = m.bin_deps.get_or_insert_with(indexmap::IndexMap::new);
        bin_deps.insert(
            pkg_name_str.clone(),
            BinaryDependencyInfo {
                common: SourceDependencyInfo::Simple(version.clone()),
                ..Default::default()
            },
        );
    } else {
        if m.deps.contains_key(&pkg_name_str) {
            eprintln!(
                "{}: dependency `{pkg_name_str}` already exists, `moon add` will not update it. \
                To update the dependency, run `moon add --upgrade {pkg_name_str}@<version>` or `moon add --upgrade {pkg_name_str}` for the latest version.",
                "Warning".yellow().bold(),
            );
            return Ok(0);
        }

        m.deps.insert(
            pkg_name_str.clone(),
            SourceDependencyInfo::Simple(version.clone()),
        );
    }

    if upgrade {
        if module_dir.join(MOON_MOD).exists() {
            patch_module_dsl_to_file(
                module_dir,
                MoonModPatch::UpdateImportItems(IndexMap::from([(pkg_name_str, version.clone())])),
            )?;
        } else {
            let new_j = convert_module_to_mod_json(m);
            write_module_json_to_file(&new_j, module_dir)?;
        }
        return Ok(0);
    }

    let m = Arc::new(m);
    let roots = roots_for_selected_module(
        project_root,
        module_dir,
        Arc::clone(&m),
        project_manifest_path,
    )?;
    install_impl(
        mooncakes_dir,
        roots,
        SyncOutputOptions::new(quiet, true),
        false,
        false,
        true,
    )?;

    if module_dir.join(MOON_MOD).exists() {
        let patch = if bin {
            MoonModPatch::Rewrite(convert_module_to_mod_json(Arc::unwrap_or_clone(m)))
        } else {
            MoonModPatch::InsertImportItem {
                name: pkg_name_str,
                version: version.clone(),
            }
        };
        patch_module_dsl_to_file(module_dir, patch)?;
    } else {
        let new_j = convert_module_to_mod_json(Arc::unwrap_or_clone(m));
        write_module_json_to_file(&new_j, module_dir)?;
    }

    Ok(0)
}
