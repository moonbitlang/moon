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

//! Project preparation: module dependency sync, package discovery, and package resolution.
//!
//! Module sync selects module sources and versions and makes them available on disk.
//! Discovery produces [`DiscoveredProject`]; package resolution adds validated
//! package relationships to produce [`ResolvedProject`].
//! Dependency-directory mutation remains explicit in the sync step.

use std::{
    collections::{BTreeMap, HashSet},
    ops::Deref,
    path::Path,
};

use log::{debug, info};

use mooncake::pkg::sync::{SyncOutputOptions, auto_sync, auto_sync_for_single_file_rr};
use moonutil::{
    cache::CacheRoot,
    cli_support::AutoSyncFlags,
    manifest::MoonMod,
    project::{PackageDirs, WorkspaceEnv},
    resolution::{DirSyncResult, ModuleDependencyGraph, ModuleId},
    target::TargetBackend,
    user_log::UserLog,
};
use tracing::instrument;

use crate::single_file::SingleFileConfig;

use crate::discover::special_case::inject_core_coverage_into_builtin;
use crate::special_cases::CORE_MODULE_TUPLE;
use crate::{
    discover::{DiscoverError, DiscoverResult, discover_packages},
    pkg_solve::{self, PackageRelations},
};

/// A project's modules and packages before solving package dependencies.
#[derive(Debug, Clone)]
pub struct DiscoveredProject {
    /// Selected modules and their dependency edges.
    pub module_graph: ModuleDependencyGraph,
    /// Module directories
    pub module_dirs: DirSyncResult,
    /// Package declarations and file sets.
    pub pkg_dirs: DiscoverResult,
    /// Keep graph injection consistent with coverage sources added during discovery.
    pub(crate) enable_coverage: bool,
}

/// A project with both module dependencies and package relationships resolved.
///
/// Keeps package relationships paired with the declarations used to resolve them.
#[derive(Debug, Clone)]
pub struct ResolvedProject {
    pub discovered: DiscoveredProject,
    /// Each backend owns its imports, virtual-package associations, and support.
    pub package_relations: BTreeMap<TargetBackend, PackageRelations>,
}

/// A resolved project provides read-only access to its modules and packages.
impl Deref for ResolvedProject {
    type Target = DiscoveredProject;

    fn deref(&self) -> &Self::Target {
        &self.discovered
    }
}

impl DiscoveredProject {
    /// Resolve exactly the requested backends, returning any error immediately.
    /// No `ResolvedProject` is constructed until every requested graph is valid.
    #[instrument(skip_all)]
    pub fn resolve_packages(
        self,
        backends: &[TargetBackend],
        user_log: &UserLog,
    ) -> Result<ResolvedProject, ProjectPreparationError> {
        let mut package_relations = BTreeMap::new();
        let mut warnings = HashSet::new();
        let (graph_log, capture) = UserLog::captured(log::LevelFilter::Warn);
        for &backend in backends {
            if package_relations.contains_key(&backend) {
                continue;
            }
            let result = pkg_solve::resolve_packages(
                &self.module_graph,
                &self.pkg_dirs,
                self.enable_coverage,
                backend,
                &graph_log,
            );
            for entry in capture.take() {
                if warnings.insert(entry.message.clone()) {
                    user_log.warn(entry.message);
                }
            }
            let relations =
                result.map_err(|e| ProjectPreparationError::PackageResolutionError(Box::new(e)))?;
            info!("Package dependency resolution completed successfully");
            debug!(
                "Package dependency graph has {} nodes",
                relations.dep_graph.node_count()
            );
            package_relations.insert(backend, relations);
        }
        Ok(ResolvedProject {
            discovered: self,
            package_relations,
        })
    }

    /// Returns the input/root modules of the project.
    ///
    /// This is a role in the current resolution graph, not a check of
    /// `ModuleSourceKind::Local`.
    pub fn local_modules(&self) -> &[ModuleId] {
        self.module_graph.input_module_ids()
    }

    pub fn module_info(&self, id: ModuleId) -> &MoonMod {
        self.pkg_dirs.module_info(id)
    }
}

/// Settings shared by module sync, package discovery, and package resolution.
#[derive(Debug)]
pub struct ProjectPreparationConfig {
    sync_flags: AutoSyncFlags,
    sync_output: SyncOutputOptions,
    dependency_source_cache: CacheRoot,
    no_std: bool,
    /// Gate coverage injection in pkg_solve
    pub enable_coverage: bool,
    workspace_env: WorkspaceEnv,
}

impl ProjectPreparationConfig {
    /// Create project preparation settings with the requested module sync policy
    /// and defaults for the remaining options.
    pub fn new_with_load_defaults(
        frozen: bool,
        no_std: bool,
        enable_coverage: bool,
        workspace_env: WorkspaceEnv,
    ) -> Self {
        Self {
            sync_flags: AutoSyncFlags { frozen },
            sync_output: SyncOutputOptions::default(),
            dependency_source_cache: CacheRoot::Disabled,
            no_std,
            enable_coverage,
            workspace_env,
        }
    }

    /// Create project preparation settings with the given sync and build flags.
    pub fn new(
        sync_flags: AutoSyncFlags,
        no_std: bool,
        enable_coverage: bool,
        workspace_env: WorkspaceEnv,
    ) -> Self {
        Self {
            sync_flags,
            sync_output: SyncOutputOptions::default(),
            dependency_source_cache: CacheRoot::Disabled,
            no_std,
            enable_coverage,
            workspace_env,
        }
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

/// Failures from module sync, package discovery, or package resolution.
#[derive(Debug, thiserror::Error)]
pub enum ProjectPreparationError {
    #[error("Failed to resolve the module dependency graph")]
    SyncModulesError(#[source] anyhow::Error),

    #[error("Failed when discovering packages")]
    DiscoverError(#[from] DiscoverError),

    #[error("Failed to solve package relationship")]
    PackageResolutionError(#[source] Box<pkg_solve::PackageResolutionError>),
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
        cfg.no_std,
        cfg.workspace_env.clone(),
    )
    .map_err(ProjectPreparationError::SyncModulesError)?;
    info!("Module dependency resolution completed successfully");
    debug!("Resolved {} modules", module_graph.module_count());

    Ok((module_graph, dir_sync_result))
}

/// Resolve package relationships for the requested backends from synced dependencies.
#[instrument(skip_all)]
pub fn prepare_synced_project(
    cfg: &ProjectPreparationConfig,
    synced_dependencies: (ModuleDependencyGraph, DirSyncResult),
    backends: &[TargetBackend],
    user_log: &UserLog,
) -> Result<ResolvedProject, ProjectPreparationError> {
    discover_synced_project(cfg, synced_dependencies, user_log)?
        .resolve_packages(backends, user_log)
}

/// Discover packages from already synced dependencies without solving imports.
#[instrument(skip_all)]
pub fn discover_synced_project(
    cfg: &ProjectPreparationConfig,
    synced_dependencies: (ModuleDependencyGraph, DirSyncResult),
    user_log: &UserLog,
) -> Result<DiscoveredProject, ProjectPreparationError> {
    let (module_graph, dir_sync_result) = synced_dependencies;

    let mut discover_result = discover_packages(&module_graph, &dir_sync_result, user_log)?;
    let main_is_core = {
        let ids = module_graph.input_module_ids();
        ids.len() == 1 && *module_graph.module_source(ids[0]).name() == CORE_MODULE_TUPLE
    };
    if cfg.enable_coverage && main_is_core {
        // Gate coverage bundling (coverage -> builtin) behind both flag and main-module check
        inject_core_coverage_into_builtin(&module_graph, &mut discover_result);
    }

    info!(
        "Package discovery completed, found {} packages",
        discover_result.package_count()
    );

    Ok(DiscoveredProject {
        module_graph,
        module_dirs: dir_sync_result,
        pkg_dirs: discover_result,
        enable_coverage: cfg.enable_coverage,
    })
}

/// Discover a standalone project from its already interpreted configuration.
/// `source_file` must be the absolute invoked path from
/// `SingleFilePackageDirs::input_path`, preserving a file symlink's own filename.
#[instrument(skip_all)]
pub fn discover_single_file_project(
    cfg: &ProjectPreparationConfig,
    dirs: &PackageDirs,
    source_file: &Path,
    config: SingleFileConfig,
    user_log: &UserLog,
) -> Result<DiscoveredProject, ProjectPreparationError> {
    let (module_graph, dir_sync_result) = auto_sync_for_single_file_rr(
        dirs,
        &cfg.sync_flags,
        &config.deps,
        cfg.sync_output,
        &cfg.dependency_source_cache,
        user_log,
    )
    .map_err(ProjectPreparationError::SyncModulesError)?;
    let mut discover_result = discover_packages(&module_graph, &dir_sync_result, user_log)?;
    crate::discover::synth::build_synth_single_file_package(
        source_file,
        config.source_kind,
        &module_graph,
        &mut discover_result,
        config.imports,
    )?;

    Ok(DiscoveredProject {
        module_graph,
        module_dirs: dir_sync_result,
        pkg_dirs: discover_result,
        enable_coverage: cfg.enable_coverage,
    })
}
