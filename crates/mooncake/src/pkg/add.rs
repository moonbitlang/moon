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

use colored::Colorize;
use moonutil::common::{read_module_desc_file_in_dir, write_module_json_to_file, MOONBITLANG_CORE};
use moonutil::dependency::{BinaryDependencyInfo, SourceDependencyInfo};
use moonutil::module::convert_module_to_mod_json;
use moonutil::mooncakes::{ModuleName, ModuleSource};
use semver::Version;
use std::path::Path;
use std::rc::Rc;

use crate::registry::{self, Registry, RegistryList};
use crate::resolver::resolve_single_root_with_defaults;

/// Add a dependency
#[derive(Debug, clap::Parser)]
pub struct AddSubcommand {
    /// The package path to add
    pub package_path: String,

    /// Whether to add the dependency as a binary
    #[clap(long)]
    pub bin: bool,
}

pub fn add_latest(
    source_dir: &Path,
    target_dir: &Path,
    pkg_name: &ModuleName,
    bin: bool,
    quiet: bool,
) -> anyhow::Result<i32> {
    if pkg_name.to_string() == MOONBITLANG_CORE {
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
            anyhow::anyhow!(
                "could not find the latest version of {}. Please consider running `moon update` to update the index.",
                pkg_name.to_string()
            )
        })?
        .version
        .clone()
        .unwrap();
    add(
        source_dir,
        target_dir,
        pkg_name,
        bin,
        &latest_version,
        quiet,
    )
}

#[test]
fn test_module_name() {
    let core_name = MOONBITLANG_CORE.parse::<ModuleName>().unwrap();
    assert_eq!(MOONBITLANG_CORE, core_name.to_string());
}

pub fn add(
    source_dir: &Path,
    _target_dir: &Path,
    pkg_name: &ModuleName,
    bin: bool,
    version: &Version,
    quiet: bool,
) -> anyhow::Result<i32> {
    let mut m = read_module_desc_file_in_dir(source_dir)?;

    if pkg_name.to_string() == MOONBITLANG_CORE {
        eprintln!(
            "{}: no need to add `{}` as dependency",
            "Warning".yellow().bold(),
            MOONBITLANG_CORE
        );
        std::process::exit(0);
    }

    if bin {
        let bin_deps = m.bin_deps.get_or_insert_with(indexmap::IndexMap::new);
        bin_deps.insert(
            pkg_name.to_string(),
            BinaryDependencyInfo {
                version: moonutil::version::as_caret_version_req(version.clone()),
                ..Default::default()
            },
        );
    } else {
        m.deps.insert(
            pkg_name.to_string(),
            SourceDependencyInfo {
                version: moonutil::version::as_caret_version_req(version.clone()),
                ..Default::default()
            },
        );
    }
    let ms = ModuleSource::from_local_module(&m, source_dir).expect("Malformed module manifest");
    let registries = RegistryList::with_default_registry();
    let m = Rc::new(m);
    let result = resolve_single_root_with_defaults(&registries, ms, Rc::clone(&m))?;

    let dep_dir = crate::dep_dir::DepDir::of_source(source_dir);
    crate::dep_dir::sync_deps(&dep_dir, &registries, &result, quiet)?;

    drop(result);

    let new_j = convert_module_to_mod_json(Rc::into_inner(m).unwrap());
    write_module_json_to_file(&new_j, source_dir)?;

    Ok(0)
}
