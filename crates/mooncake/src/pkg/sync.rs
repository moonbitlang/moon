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

//! Sync dependencies with mod/pkg definition

use std::sync::Arc;

use indexmap::IndexMap;
use moonutil::{
    cache::CacheRoot,
    cli_support::AutoSyncFlags,
    manifest::{MoonMod, read_module_desc_file_in_dir},
    project::{MoonWork, PackageDirs, ProjectManifest, WorkspaceEnv, WorkspaceLayout},
    resolution::{
        DirSyncResult, ModuleDependencyGraph, ModuleSource, ResolvedModule, ResolvedRootModules,
    },
    user_log::UserLog,
};
use semver::Version;

/// User-visible output policy while synchronizing dependencies.
#[derive(Debug, Clone, Copy, Default)]
pub struct SyncOutputOptions {
    /// Suppress dependency acquisition progress below error level.
    pub quiet: bool,
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
) -> anyhow::Result<(ModuleDependencyGraph, DirSyncResult, Option<MoonWork>)> {
    if let ProjectManifest::Workspace(workspace) = &dirs.project_manifest
        && !matches!(workspace_env, WorkspaceEnv::Off)
    {
        return resolve_workspace_sync(dirs, cli, output_options, user_log, no_std, workspace);
    }

    let module = read_module_desc_file_in_dir(&dirs.source_dir)?;
    let module = Arc::new(module);
    let source = ModuleSource::from_local_module(&module, &dirs.source_dir)?;
    let (roots, _) = ResolvedModule::only_one_module(source, module);

    let (module_graph, sync_result) = super::install::install_impl(
        dirs,
        roots,
        output_options,
        user_log,
        cli.dont_sync(),
        no_std,
        &CacheRoot::Disabled,
    )?;
    log::debug!("Dir sync result: {:?}", sync_result);
    Ok((module_graph, sync_result, None))
}

fn resolve_workspace_sync(
    dirs: &PackageDirs,
    cli: &AutoSyncFlags,
    output_options: SyncOutputOptions,
    user_log: &UserLog,
    no_std: bool,
    workspace: &WorkspaceLayout,
) -> anyhow::Result<(ModuleDependencyGraph, DirSyncResult, Option<MoonWork>)> {
    let mut roots = ResolvedRootModules::with_key();
    for member_dir in workspace.members() {
        let module = read_module_desc_file_in_dir(member_dir)?;
        let module = Arc::new(module);
        let source = ModuleSource::from_local_module(&module, member_dir)?;
        roots.insert(ResolvedModule::new(source, module));
    }

    let (module_graph, sync_result) = super::install::install_impl(
        dirs,
        roots,
        output_options,
        user_log,
        cli.dont_sync(),
        no_std,
        &CacheRoot::Disabled,
    )?;
    log::debug!("Dir sync result: {:?}", sync_result);
    Ok((
        module_graph,
        sync_result,
        Some(workspace.manifest().clone()),
    ))
}

pub fn auto_sync_for_single_file_rr(
    dirs: &PackageDirs,
    sync_flags: &AutoSyncFlags,
    front_matter_deps: Option<&IndexMap<String, moonutil::dependency::SourceDependencyInfo>>,
    output_options: SyncOutputOptions,
    source_cache: &CacheRoot,
    user_log: &UserLog,
) -> anyhow::Result<(ModuleDependencyGraph, DirSyncResult)> {
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
    let ms = ModuleSource::single_file(&m, &dirs.source_dir)?;
    let (roots, _) = ResolvedModule::only_one_module(ms, Arc::clone(&m));

    let (module_graph, dir_sync_result) = super::install::install_impl(
        dirs,
        roots,
        output_options,
        user_log,
        sync_flags.dont_sync(),
        false,
        source_cache,
    )?;

    log::debug!("Dir sync result: {:?}", dir_sync_result);
    Ok((module_graph, dir_sync_result))
}
