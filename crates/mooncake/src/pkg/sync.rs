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

pub use moonutil::{
    cache::{CacheKind, CacheRoot},
    resolution::{
        DirSyncResult, ModuleDependencyGraph, ModuleId, ModuleSource, ModuleSourceKind,
        ResolvedModule, ResolvedRootModules,
    },
};
use moonutil::{
    manifest::read_module_desc_file_in_dir,
    project::{PackageDirs, ProjectManifest},
    user_log::UserLog,
};

use super::prepare::prepare_build_modules;
use crate::{
    dependency_source, registry,
    resolver::{ModuleResolutionConfig, resolve_modules},
};

/// Policy for module resolution and dependency source acquisition.
#[derive(Debug, Clone, Copy, Default)]
pub struct SyncOptions {
    /// Require locally available dependency metadata and sources.
    pub frozen: bool,
    /// Suppress dependency acquisition progress below error level.
    /// Module resolution warnings still use the caller's user-log level.
    pub quiet: bool,
    /// Limit concurrent dependency acquisition jobs.
    pub jobs: Option<usize>,
}

/// Load root modules from the selected project and synchronize their dependencies
/// into the project's `.mooncakes` directory. The selected `ProjectManifest` is
/// authoritative; workspace selection has already been resolved by discovery.
pub fn auto_sync(
    dirs: &PackageDirs,
    options: &SyncOptions,
    user_log: &UserLog,
) -> anyhow::Result<(ModuleDependencyGraph, DirSyncResult)> {
    let roots = match &dirs.project_manifest {
        ProjectManifest::Workspace(workspace) => {
            let mut roots = ResolvedRootModules::with_key();
            for member_dir in workspace.members() {
                let module = Arc::new(read_module_desc_file_in_dir(member_dir)?);
                let source = ModuleSource::from_local_module(&module, member_dir)?;
                roots.insert(ResolvedModule::new(source, module));
            }
            roots
        }
        ProjectManifest::Module(_) | ProjectManifest::None => {
            let module = Arc::new(read_module_desc_file_in_dir(&dirs.source_dir)?);
            let source = ModuleSource::from_local_module(&module, &dirs.source_dir)?;
            ResolvedModule::only_one_module(source, module).0
        }
    };

    let (module_graph, sync_result) = sync_roots(
        roots,
        &dirs.mooncakes_dir,
        &CacheRoot::Disabled,
        options,
        user_log,
    )?;
    log::debug!("Dir sync result: {:?}", sync_result);
    Ok((module_graph, sync_result))
}

/// Resolve and acquire dependencies for root declarations supplied by the caller.
/// Root declarations are used directly, without reloading their manifests.
/// The returned graph and source directories can be passed to package discovery.
///
/// This applies the same installed-core policy and configured registry as project
/// synchronization. Disable `source_cache` to acquire sources in `mooncakes_dir`,
/// or provide a shared source cache for immutable dependencies.
///
/// TODO: support registry config
pub fn sync_roots(
    roots: ResolvedRootModules,
    mooncakes_dir: &Path,
    source_cache: &CacheRoot,
    options: &SyncOptions,
    user_log: &UserLog,
) -> anyhow::Result<(ModuleDependencyGraph, DirSyncResult)> {
    let module_graph = prepare_build_modules(roots)?;

    let registry = if options.frozen {
        registry::default_registry(user_log).for_frozen_dependencies()
    } else {
        registry::default_registry(user_log)
    };
    let module_resolution_config = ModuleResolutionConfig {
        registry: &registry,
        warn_deprecated: true,
    };

    let res = resolve_modules(&module_resolution_config, module_graph, user_log)?;
    let dependency_user_log = if options.quiet {
        user_log.with_level(log::LevelFilter::Error)
    } else {
        user_log.clone()
    };
    let dependency_source =
        dependency_source::select(mooncakes_dir, source_cache, &res, options.jobs)?;
    let dependency_paths =
        dependency_source.ensure(&registry, &res, options.frozen, &dependency_user_log)?;

    Ok((res, dependency_paths))
}
