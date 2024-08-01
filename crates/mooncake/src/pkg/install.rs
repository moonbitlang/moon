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

use crate::{dep_dir::DepDir, resolver::resolve_single_root_with_defaults};

use anyhow::Context;
use moonutil::{
    common::read_module_desc_file_in_dir,
    mooncakes::{result::ResolvedEnv, ModuleSource, RegistryConfig},
};
use std::{path::Path, rc::Rc};

pub fn install(
    source_dir: &Path,
    _target_dir: &Path,
    registry_config: &RegistryConfig,
    quiet: bool,
) -> anyhow::Result<i32> {
    install_impl(source_dir, registry_config, quiet, false).map(|_| 0)
}

pub(crate) fn install_impl(
    source_dir: &Path,
    _registry_config: &RegistryConfig,
    quiet: bool,
    dont_sync: bool,
) -> anyhow::Result<(ResolvedEnv, DepDir)> {
    let m = read_module_desc_file_in_dir(source_dir)?;
    let m = Rc::new(m);
    let registry = crate::registry::RegistryList::with_default_registry();
    let ms = ModuleSource::from_local_module(&m, source_dir).expect("Malformed module manifest");
    let res = resolve_single_root_with_defaults(&registry, ms, Rc::clone(&m))?;
    let dep_dir = crate::dep_dir::DepDir::of_source(source_dir);
    if !dont_sync {
        crate::dep_dir::sync_deps(&dep_dir, &registry, &res, quiet)
            .context("When installing packages")?;
    }
    Ok((res, dep_dir))
}
