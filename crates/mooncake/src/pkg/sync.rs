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

//! Resolve module dependencies and synchronize their source directories.

use std::{path::Path, sync::Arc};

use indexmap::IndexMap;
use moonutil::{
    cache::CacheRoot,
    manifest::{MoonMod, read_module_desc_file_in_dir},
    project::{PackageDirs, ProjectManifest, WorkspaceLayout},
    resolution::{
        DirSyncResult, ModuleDependencyGraph, ModuleSource, ResolvedModule, ResolvedRootModules,
    },
    user_log::UserLog,
};
use semver::Version;
use serde::{Deserialize, Serialize};

use super::prepare::prepare_build_modules;
use crate::{
    dependency_source, registry,
    resolver::{ModuleResolutionConfig, resolve_modules},
};

/// User-visible output policy while synchronizing dependencies.
#[derive(Debug, Clone, Copy, Default)]
pub struct SyncOutputOptions {
    /// Suppress dependency acquisition progress below error level.
    pub quiet: bool,
}

#[derive(Debug, clap::Parser, Serialize, Deserialize, Clone)]
#[clap(next_help_heading = "Manifest Options")]
pub struct AutoSyncFlags {
    /// Do not sync dependencies, assuming local dependencies are up-to-date
    #[clap(long)]
    pub frozen: bool,
}

impl AutoSyncFlags {
    pub fn dont_sync(&self) -> bool {
        self.frozen
    }
}

/// Given the specified source directory, resolve the module dependency relation
/// and their directories. The selected `ProjectManifest` is authoritative;
/// workspace selection has already been resolved by project discovery.
///
/// TODO: support registry config
pub fn auto_sync(
    dirs: &PackageDirs,
    cli: &AutoSyncFlags,
    output_options: SyncOutputOptions,
    user_log: &UserLog,
    jobs: Option<usize>,
) -> anyhow::Result<(ModuleDependencyGraph, DirSyncResult)> {
    if let ProjectManifest::Workspace(workspace) = &dirs.project_manifest {
        return resolve_workspace_sync(dirs, cli, output_options, user_log, workspace, jobs);
    }

    let module = read_module_desc_file_in_dir(&dirs.source_dir)?;
    let module = Arc::new(module);
    let source = ModuleSource::from_local_module(&module, &dirs.source_dir)?;
    let (roots, _) = ResolvedModule::only_one_module(source, module);

    let (module_graph, sync_result) = sync_roots(
        &dirs.mooncakes_dir,
        roots,
        output_options,
        user_log,
        cli.dont_sync(),
        &CacheRoot::Disabled,
        jobs,
    )?;
    log::debug!("Dir sync result: {:?}", sync_result);
    Ok((module_graph, sync_result))
}

fn resolve_workspace_sync(
    dirs: &PackageDirs,
    cli: &AutoSyncFlags,
    output_options: SyncOutputOptions,
    user_log: &UserLog,
    workspace: &WorkspaceLayout,
    jobs: Option<usize>,
) -> anyhow::Result<(ModuleDependencyGraph, DirSyncResult)> {
    let mut roots = ResolvedRootModules::with_key();
    for member_dir in workspace.members() {
        let module = read_module_desc_file_in_dir(member_dir)?;
        let module = Arc::new(module);
        let source = ModuleSource::from_local_module(&module, member_dir)?;
        roots.insert(ResolvedModule::new(source, module));
    }

    let (module_graph, sync_result) = sync_roots(
        &dirs.mooncakes_dir,
        roots,
        output_options,
        user_log,
        cli.dont_sync(),
        &CacheRoot::Disabled,
        jobs,
    )?;
    log::debug!("Dir sync result: {:?}", sync_result);
    Ok((module_graph, sync_result))
}

pub fn auto_sync_for_single_file_rr(
    dirs: &PackageDirs,
    sync_flags: &AutoSyncFlags,
    front_matter_deps: Option<&IndexMap<String, moonutil::dependency::SourceDependencyInfo>>,
    output_options: SyncOutputOptions,
    source_cache: &CacheRoot,
    jobs: Option<usize>,
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

    let (module_graph, dir_sync_result) = sync_roots(
        &dirs.mooncakes_dir,
        roots,
        output_options,
        user_log,
        sync_flags.dont_sync(),
        source_cache,
        jobs,
    )?;

    log::debug!("Dir sync result: {:?}", dir_sync_result);
    Ok((module_graph, dir_sync_result))
}

/// Resolve and acquire dependencies for the supplied root modules.
pub(super) fn sync_roots(
    mooncakes_dir: &Path,
    roots: ResolvedRootModules,
    output_options: SyncOutputOptions,
    user_log: &UserLog,
    dont_sync: bool,
    source_cache: &CacheRoot,
    jobs: Option<usize>,
) -> anyhow::Result<(ModuleDependencyGraph, DirSyncResult)> {
    let module_graph = prepare_build_modules(roots)?;

    let registry = if dont_sync {
        registry::default_registry(user_log).for_frozen_dependencies()
    } else {
        registry::default_registry(user_log)
    };
    let module_resolution_config = ModuleResolutionConfig {
        registry: &registry,
        warn_deprecated: true,
    };

    let res = resolve_modules(&module_resolution_config, module_graph, user_log)?;
    let dependency_user_log = if output_options.quiet {
        user_log.with_level(log::LevelFilter::Error)
    } else {
        user_log.clone()
    };
    let dependency_source = dependency_source::select(mooncakes_dir, source_cache, &res, jobs)?;
    let dependency_paths =
        dependency_source.ensure(&registry, &res, dont_sync, &dependency_user_log)?;

    Ok((res, dependency_paths))
}
