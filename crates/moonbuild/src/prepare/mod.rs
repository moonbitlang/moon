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

//! Dependency and script preparation before package discovery and resolution.

mod single_file;
pub use single_file::discover_single_file_project;

use moonbuild_rupes_recta::{DiscoveryConfig, resolve::ProjectResolutionError};
use mooncake::pkg::sync::{SyncOutputOptions, auto_sync};
use moonutil::{
    cache::CacheRoot,
    cli_support::AutoSyncFlags,
    project::{PackageDirs, WorkspaceEnv},
    resolution::{DirSyncResult, ModuleDependencyGraph},
    user_log::UserLog,
};
use tracing::{debug, info, instrument};

/// Settings for dependency preparation and subsequent package discovery.
#[derive(Debug)]
pub struct ProjectPreparationConfig {
    sync_flags: AutoSyncFlags,
    sync_output: SyncOutputOptions,
    dependency_source_cache: CacheRoot,
    workspace_env: WorkspaceEnv,
    pub discovery: DiscoveryConfig,
}

impl ProjectPreparationConfig {
    /// Create project preparation settings with the requested module sync policy
    /// and defaults for the remaining options.
    pub fn new_with_load_defaults(
        frozen: bool,
        enable_coverage: bool,
        workspace_env: WorkspaceEnv,
    ) -> Self {
        Self {
            sync_flags: AutoSyncFlags { frozen },
            sync_output: SyncOutputOptions::default(),
            dependency_source_cache: CacheRoot::Disabled,
            discovery: DiscoveryConfig {
                enable_coverage,
                jobs: None,
            },
            workspace_env,
        }
    }

    /// Create project preparation settings with the given sync and build flags.
    pub fn new(
        sync_flags: AutoSyncFlags,
        enable_coverage: bool,
        workspace_env: WorkspaceEnv,
    ) -> Self {
        Self {
            sync_flags,
            sync_output: SyncOutputOptions::default(),
            dependency_source_cache: CacheRoot::Disabled,
            discovery: DiscoveryConfig {
                enable_coverage,
                jobs: None,
            },
            workspace_env,
        }
    }

    /// Use the same parallelism limit for dependency preparation and package discovery.
    pub fn with_jobs(mut self, jobs: Option<usize>) -> Self {
        self.discovery.jobs = jobs;
        self
    }

    pub fn with_sync_output(mut self, sync_output: SyncOutputOptions) -> Self {
        self.sync_output = sync_output;
        self
    }

    pub fn with_dependency_source_cache(mut self, cache: CacheRoot) -> Self {
        self.dependency_source_cache = cache;
        self
    }
}

/// Failures while preparing dependencies and script declarations for discovery.
#[derive(Debug, thiserror::Error)]
pub enum ProjectPreparationError {
    #[error("Failed to resolve the module dependency graph")]
    SyncModulesError(#[source] anyhow::Error),

    #[error("Failed to parse single file front matter configuration")]
    SingleFileParseError(#[source] anyhow::Error),

    #[error(transparent)]
    ProjectResolution(#[from] ProjectResolutionError),
}

/// Resolve module sources and versions and synchronize their directories for
/// package discovery. This does not resolve package imports.
#[instrument(skip_all)]
pub fn sync_module_dependencies(
    cfg: &ProjectPreparationConfig,
    dirs: &PackageDirs,
    user_log: &UserLog,
) -> Result<(ModuleDependencyGraph, DirSyncResult), ProjectPreparationError> {
    info!(
        "Starting dependency sync for source directory: {}",
        dirs.source_dir.display()
    );
    debug!("Resolve config: sync_flags={:?}", cfg.sync_flags);

    let (module_graph, dir_sync_result, _) = auto_sync(
        dirs,
        &cfg.sync_flags,
        cfg.sync_output,
        user_log,
        cfg.workspace_env.clone(),
        cfg.discovery.jobs,
    )
    .map_err(ProjectPreparationError::SyncModulesError)?;
    info!("Module dependency resolution completed successfully");
    debug!("Resolved {} modules", module_graph.module_count());

    Ok((module_graph, dir_sync_result))
}
