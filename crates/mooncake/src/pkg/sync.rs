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
    build_options::{MoonbuildOpt, MooncOpt},
    cache::CacheRoot,
    cli_support::AutoSyncFlags,
    front_matter::MbtMdHeader,
    manifest::{MoonMod, read_module_desc_file_in_dir},
    project::{
        MoonWork, PackageDirs, ProjectManifest, WorkspaceEnv, canonical_workspace_module_dirs,
        read_workspace_file,
    },
    resolution::{DirSyncResult, ModuleSource, ResolvedEnv, ResolvedModule, ResolvedRootModules},
    user_log::UserLog,
};
use semver::Version;

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncOutputOptions {
    capture_child_output: bool,
}

impl SyncOutputOptions {
    pub fn with_captured_child_output(mut self, capture: bool) -> Self {
        self.capture_child_output = capture;
        self
    }

    pub fn capture_child_output(self) -> bool {
        self.capture_child_output
    }
}

/// Given the specified source directory, resolve the module dependency relation
/// and their directories
///
/// TODO: support registry config
pub fn auto_sync(
    dirs: &PackageDirs,
    cli: &AutoSyncFlags,
    output_options: SyncOutputOptions,
    user_log: &UserLog,
    no_std: bool,
    workspace_env: WorkspaceEnv,
    include_bin_deps: bool,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult, Option<MoonWork>)> {
    if let ProjectManifest::Workspace(project_manifest) = &dirs.project_manifest
        && !matches!(workspace_env, WorkspaceEnv::Off)
    {
        let workspace = read_workspace_file(project_manifest, user_log)?;
        return resolve_workspace_sync(
            dirs,
            cli,
            output_options,
            user_log,
            no_std,
            workspace,
            include_bin_deps,
        );
    }

    let mut module = read_module_desc_file_in_dir(&dirs.source_dir)?;
    if !include_bin_deps {
        module.bin_deps = None;
    }
    let module = Arc::new(module);
    let source = ModuleSource::from_local_module(&module, &dirs.source_dir);
    let (roots, _) = ResolvedModule::only_one_module(source, module);

    let (resolved_env, sync_result) = super::install::install_impl(
        dirs,
        roots,
        output_options,
        user_log,
        cli.dont_sync(),
        no_std,
        &CacheRoot::Disabled,
    )?;
    log::debug!("Dir sync result: {:?}", sync_result);
    Ok((resolved_env, sync_result, None))
}

fn resolve_workspace_sync(
    dirs: &PackageDirs,
    cli: &AutoSyncFlags,
    output_options: SyncOutputOptions,
    user_log: &UserLog,
    no_std: bool,
    workspace: MoonWork,
    include_bin_deps: bool,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult, Option<MoonWork>)> {
    let ProjectManifest::Workspace(project_manifest) = &dirs.project_manifest else {
        unreachable!("workspace sync requires a workspace manifest");
    };
    let workspace_root = project_manifest
        .parent()
        .context("workspace manifest path has no parent directory")?;
    let mut roots = ResolvedRootModules::with_key();
    for member_dir in canonical_workspace_module_dirs(workspace_root, &workspace)? {
        let mut module = read_module_desc_file_in_dir(&member_dir)?;
        if !include_bin_deps {
            module.bin_deps = None;
        }
        let module = Arc::new(module);
        let source = ModuleSource::from_local_module(&module, &member_dir);
        roots.insert(ResolvedModule::new(source, module));
    }

    let (resolved_env, sync_result) = super::install::install_impl(
        dirs,
        roots,
        output_options,
        user_log,
        cli.dont_sync(),
        no_std,
        &CacheRoot::Disabled,
    )?;
    log::debug!("Dir sync result: {:?}", sync_result);
    Ok((resolved_env, sync_result, Some(workspace)))
}

pub fn auto_sync_for_single_mbt_md(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    mooncake_bin_dir: &Path,
    mooncakes_dir: &Path,
    front_matter_config: Option<MbtMdHeader>,
    user_log: &UserLog,
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
        name: moonutil::constants::SINGLE_FILE_TEST_MODULE.to_string(),
        version: Some(Version::new(0, 0, 1)),
        deps,
        warn_list: moonc_opt.build_opt.warn_list.clone(),
        ..Default::default()
    });
    let ms = ModuleSource::single_file(&m, &moonbuild_opt.source_dir);
    let (roots, _) = ResolvedModule::only_one_module(ms, Arc::clone(&m));
    let dirs = PackageDirs {
        source_dir: moonbuild_opt.source_dir.clone(),
        target_dir: moonbuild_opt.target_dir.clone(),
        mooncake_bin_dir: mooncake_bin_dir.to_path_buf(),
        mooncakes_dir: mooncakes_dir.to_path_buf(),
        project_manifest: ProjectManifest::None,
    };

    let (resolved_env, dir_sync_result) = super::install::install_impl(
        &dirs,
        roots,
        SyncOutputOptions::default(),
        user_log,
        dont_sync,
        false,
        &CacheRoot::Disabled,
    )?;
    log::debug!("Dir sync result: {:?}", dir_sync_result);
    Ok((resolved_env, dir_sync_result, m))
}

pub fn auto_sync_for_single_file_rr(
    dirs: &PackageDirs,
    sync_flags: &AutoSyncFlags,
    front_matter_deps: Option<&IndexMap<String, moonutil::dependency::SourceDependencyInfo>>,
    output_options: SyncOutputOptions,
    source_cache: &CacheRoot,
    user_log: &UserLog,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult)> {
    let mut synth_deps = IndexMap::new();
    if let Some(deps_map) = front_matter_deps {
        for (k, v) in deps_map.iter() {
            synth_deps.insert(k.clone(), v.clone());
        }
    }

    let m = Arc::new(MoonMod {
        name: moonutil::constants::SINGLE_FILE_TEST_MODULE.to_string(),
        version: Some(Version::new(0, 0, 1)),
        deps: synth_deps,
        ..Default::default()
    });
    let ms = ModuleSource::single_file(&m, &dirs.source_dir);
    let (roots, _) = ResolvedModule::only_one_module(ms, Arc::clone(&m));

    let (resolved_env, dir_sync_result) = super::install::install_impl(
        dirs,
        roots,
        output_options,
        user_log,
        sync_flags.dont_sync(),
        false,
        source_cache,
    )?;

    log::debug!("Dir sync result: {:?}", dir_sync_result);
    Ok((resolved_env, dir_sync_result))
}
