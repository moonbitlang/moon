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

//! Sync dependencies with mod/pkg definition

use std::path::Path;

use indexmap::IndexMap;
use moonutil::{
    common::{MbtMdHeader, MoonbuildOpt, MooncOpt},
    module::MoonMod,
    mooncakes::{result::ResolvedEnv, sync::AutoSyncFlags, DirSyncResult, RegistryConfig},
};
use semver::{Version, VersionReq};

use crate::dep_dir::resolve_dep_dirs;

pub fn auto_sync(
    source_dir: &Path,
    cli: &AutoSyncFlags,
    _registry_config: &RegistryConfig,
    quiet: bool,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult)> {
    let m = moonutil::common::read_module_desc_file_in_dir(source_dir)?;
    let m = std::rc::Rc::new(m);
    let (resolved_env, dep_dir) =
        super::install::install_impl(source_dir, m, quiet, false, cli.dont_sync())?;
    let dir_sync_result = resolve_dep_dirs(&dep_dir, &resolved_env);
    log::debug!("Dir sync result: {:?}", dir_sync_result);
    Ok((resolved_env, dir_sync_result))
}

pub fn auto_sync_for_single_mbt_md(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    front_matter_config: Option<MbtMdHeader>,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult, MoonMod)> {
    let mut deps = IndexMap::new();

    if let Some(deps_map) = front_matter_config
        .as_ref()
        .and_then(|config| config.moonbit.deps.as_ref())
    {
        for (k, v) in deps_map.iter() {
            deps.insert(
                k.to_string(),
                moonutil::dependency::SourceDependencyInfo {
                    version: VersionReq::parse(v).unwrap_or_default(),
                    ..Default::default()
                },
            );
        }
    }

    let m = MoonMod {
        name: moonutil::common::SINGLE_FILE_TEST_MODULE.to_string(),
        version: Some(Version::new(0, 0, 1)),
        deps,
        warn_list: moonc_opt.build_opt.warn_list.clone(),
        alert_list: moonc_opt.build_opt.alert_list.clone(),
        ..Default::default()
    };

    let (resolved_env, dep_dir) = super::install::install_impl(
        &moonbuild_opt.source_dir,
        std::rc::Rc::new(m.clone()),
        moonbuild_opt.quiet,
        moonbuild_opt.verbose,
        // don't sync for gen-test-driver
        front_matter_config.is_none(),
    )?;
    let dir_sync_result = resolve_dep_dirs(&dep_dir, &resolved_env);
    log::debug!("Dir sync result: {:?}", dir_sync_result);
    Ok((resolved_env, dir_sync_result, m))
}
