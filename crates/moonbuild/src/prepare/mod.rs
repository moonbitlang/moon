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

use anyhow::Context;
use moonbuild_rupes_recta::{DiscoveredProject, DiscoveryConfig, resolve::ProjectResolutionError};
use mooncake::pkg::sync::{SyncOptions, auto_sync};
use moonutil::{
    cache::CacheRoot,
    project::PackageDirs,
    resolution::{DirSyncResult, ModuleDependencyGraph},
    user_log::UserLog,
};
use tracing::{debug, info, instrument};

/// Settings for dependency preparation and subsequent package discovery.
#[derive(Debug)]
pub struct ProjectPreparationConfig {
    sync: SyncOptions,
    dependency_source_cache: CacheRoot,
    enable_coverage: bool,
}

impl ProjectPreparationConfig {
    /// Create project preparation settings with the requested module sync policy
    /// and defaults for the remaining options.
    pub fn new(frozen: bool, enable_coverage: bool) -> Self {
        Self {
            sync: SyncOptions {
                frozen,
                ..Default::default()
            },
            dependency_source_cache: CacheRoot::Disabled,
            enable_coverage,
        }
    }

    /// Use the same parallelism limit for dependency preparation and package discovery.
    pub fn with_jobs(mut self, jobs: Option<usize>) -> Self {
        self.sync.jobs = jobs;
        self
    }

    /// Suppress dependency acquisition progress, retaining module resolution warnings.
    pub fn with_quiet_sync(mut self, quiet: bool) -> Self {
        self.sync.quiet = quiet;
        self
    }

    /// Use a shared source cache for single-file dependencies.
    /// Normal project dependencies remain in the project's `.mooncakes` directory.
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
fn sync_module_dependencies(
    cfg: &ProjectPreparationConfig,
    dirs: &PackageDirs,
    user_log: &UserLog,
) -> Result<(ModuleDependencyGraph, DirSyncResult), ProjectPreparationError> {
    info!(
        "Starting dependency sync for source directory: {}",
        dirs.source_dir.display()
    );
    debug!("Resolve config: sync={:?}", cfg.sync);

    let (module_graph, dir_sync_result) =
        auto_sync(dirs, &cfg.sync, user_log).map_err(ProjectPreparationError::SyncModulesError)?;
    info!("Module dependency resolution completed successfully");
    debug!("Resolved {} modules", module_graph.module_count());

    Ok((module_graph, dir_sync_result))
}

/// Synchronize module dependencies and discover packages.
/// Callers can select packages and backends before resolving package imports.
/// This step does not acquire the target-directory lock.
pub fn sync_and_discover_project(
    preparation_config: &ProjectPreparationConfig,
    dirs: &PackageDirs,
    user_log: &UserLog,
) -> anyhow::Result<DiscoveredProject> {
    std::fs::create_dir_all(&dirs.target_dir).with_context(|| {
        format!(
            "Failed to create target directory: '{}'",
            dirs.target_dir.display()
        )
    })?;
    let synced_modules = sync_module_dependencies(preparation_config, dirs, user_log)?;
    Ok(moonbuild_rupes_recta::discover_synced_project(
        &DiscoveryConfig {
            enable_coverage: preparation_config.enable_coverage,
            jobs: preparation_config.sync.jobs,
        },
        synced_modules,
        user_log,
    )?)
}
