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

use std::{path::Path, sync::Arc};

use anyhow::Context;
use indexmap::IndexMap;
use moonutil::{
    common::{MOON_MOD_JSON, MbtMdHeader, MoonbuildOpt, MooncOpt, read_module_desc_file_in_dir},
    dirs::{MOON_NO_WORKSPACE, find_ancestor_with_work},
    module::MoonMod,
    mooncakes::{
        DirSyncResult, ModuleSource,
        result::{ResolvedEnv, ResolvedModule, ResolvedRootModules},
        sync::AutoSyncFlags,
    },
    workspace::{MoonWork, canonical_workspace_module_dirs, read_workspace, read_workspace_file},
};
use semver::Version;

/// Given the specified source directory, resolve the module dependency relation
/// and their directories
///
/// TODO: support registry config
pub fn auto_sync(
    source_dir: &Path,
    mooncakes_dir: &Path,
    cli: &AutoSyncFlags,
    quiet: bool,
    no_std: bool,
    project_manifest_path: Option<&Path>,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult, Option<MoonWork>)> {
    let disable_workspace = disable_workspace_from_env();
    if let Some(project_manifest_path) = project_manifest_path {
        let manifest_dir = project_manifest_path
            .parent()
            .context("manifest path has no parent directory")?;
        let manifest_dir = if manifest_dir.as_os_str().is_empty() {
            Path::new(".")
        } else {
            manifest_dir
        };
        let manifest_dir = dunce::canonicalize(manifest_dir).with_context(|| {
            format!(
                "failed to resolve manifest directory `{}`",
                manifest_dir.display()
            )
        })?;
        if project_manifest_path
            .file_name()
            .and_then(|name| name.to_str())
            != Some(MOON_MOD_JSON)
            && !disable_workspace
        {
            let workspace_root = manifest_dir.as_path();
            let workspace = read_workspace_file(project_manifest_path)?;
            let mut roots = ResolvedRootModules::with_key();
            for member_dir in canonical_workspace_module_dirs(workspace_root, &workspace)? {
                let module = Arc::new(read_module_desc_file_in_dir(&member_dir)?);
                let source = ModuleSource::from_local_module(&module, &member_dir);
                roots.insert(ResolvedModule::new(source, module));
            }

            let (resolved_env, sync_result) = super::install::install_impl(
                mooncakes_dir,
                roots,
                quiet,
                false,
                cli.dont_sync(),
                no_std,
            )?;
            log::debug!("Dir sync result: {:?}", sync_result);
            return Ok((resolved_env, sync_result, Some(workspace)));
        } else if !disable_workspace
            && let Some(workspace_root) = find_ancestor_with_work(&manifest_dir)?
        {
            let workspace = read_workspace(&workspace_root)?.context(format!(
                "failed to parse workspace file under `{}`",
                workspace_root.display()
            ))?;
            let mut roots = ResolvedRootModules::with_key();
            for member_dir in canonical_workspace_module_dirs(&workspace_root, &workspace)? {
                let module = Arc::new(read_module_desc_file_in_dir(&member_dir)?);
                let source = ModuleSource::from_local_module(&module, &member_dir);
                roots.insert(ResolvedModule::new(source, module));
            }

            let (resolved_env, sync_result) = super::install::install_impl(
                mooncakes_dir,
                roots,
                quiet,
                false,
                cli.dont_sync(),
                no_std,
            )?;
            log::debug!("Dir sync result: {:?}", sync_result);
            return Ok((resolved_env, sync_result, Some(workspace)));
        }
    } else if !disable_workspace && let Some(workspace) = read_workspace(source_dir)? {
        let mut roots = ResolvedRootModules::with_key();
        for member_dir in canonical_workspace_module_dirs(source_dir, &workspace)? {
            let module = Arc::new(read_module_desc_file_in_dir(&member_dir)?);
            let source = ModuleSource::from_local_module(&module, &member_dir);
            roots.insert(ResolvedModule::new(source, module));
        }

        let (resolved_env, sync_result) = super::install::install_impl(
            mooncakes_dir,
            roots,
            quiet,
            false,
            cli.dont_sync(),
            no_std,
        )?;
        log::debug!("Dir sync result: {:?}", sync_result);
        return Ok((resolved_env, sync_result, Some(workspace)));
    }

    let module = Arc::new(read_module_desc_file_in_dir(source_dir)?);
    let source = ModuleSource::from_local_module(&module, source_dir);
    let (roots, _) = ResolvedModule::only_one_module(source, module);

    let (resolved_env, sync_result) =
        super::install::install_impl(mooncakes_dir, roots, quiet, false, cli.dont_sync(), no_std)?;
    log::debug!("Dir sync result: {:?}", sync_result);
    Ok((resolved_env, sync_result, None))
}

fn disable_workspace_from_env() -> bool {
    match std::env::var(MOON_NO_WORKSPACE) {
        Ok(value) => value != "0",
        Err(std::env::VarError::NotPresent) => false,
        Err(std::env::VarError::NotUnicode(_)) => true,
    }
}

pub fn auto_sync_for_single_mbt_md(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    mooncakes_dir: &Path,
    front_matter_config: Option<MbtMdHeader>,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult, Arc<MoonMod>)> {
    let mut deps = IndexMap::new();

    // don't sync for gen-test-driver
    let dont_sync = front_matter_config.is_none();

    if let Some(deps_map) =
        front_matter_config.and_then(|config| config.moonbit.unwrap_or_default().deps)
    {
        for (k, v) in deps_map.iter() {
            deps.insert(k.clone(), v.clone());
        }
    }

    let m = Arc::new(MoonMod {
        name: moonutil::common::SINGLE_FILE_TEST_MODULE.to_string(),
        version: Some(Version::new(0, 0, 1)),
        deps,
        warn_list: moonc_opt.build_opt.warn_list.clone(),
        ..Default::default()
    });
    let ms = ModuleSource::single_file(&m, &moonbuild_opt.source_dir);
    let (roots, _) = ResolvedModule::only_one_module(ms, Arc::clone(&m));

    let (resolved_env, dir_sync_result) = super::install::install_impl(
        mooncakes_dir,
        roots,
        moonbuild_opt.quiet,
        moonbuild_opt.verbose,
        dont_sync,
        false,
    )?;
    log::debug!("Dir sync result: {:?}", dir_sync_result);
    Ok((resolved_env, dir_sync_result, m))
}

pub fn auto_sync_for_single_file_rr(
    source_dir: &Path,
    mooncakes_dir: &Path,
    sync_flags: &AutoSyncFlags,
    front_matter_deps: Option<&IndexMap<String, moonutil::dependency::SourceDependencyInfo>>,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult)> {
    let mut synth_deps = IndexMap::new();
    if let Some(deps_map) = front_matter_deps {
        for (k, v) in deps_map.iter() {
            synth_deps.insert(k.clone(), v.clone());
        }
    }

    let m = Arc::new(MoonMod {
        name: moonutil::common::SINGLE_FILE_TEST_MODULE.to_string(),
        version: Some(Version::new(0, 0, 1)),
        deps: synth_deps,
        ..Default::default()
    });
    let ms = ModuleSource::single_file(&m, source_dir);
    let (roots, _) = ResolvedModule::only_one_module(ms, Arc::clone(&m));

    let (resolved_env, dir_sync_result) = super::install::install_impl(
        mooncakes_dir,
        roots,
        false,
        false,
        sync_flags.dont_sync(),
        false,
    )?;

    log::debug!("Dir sync result: {:?}", dir_sync_result);
    Ok((resolved_env, dir_sync_result))
}
