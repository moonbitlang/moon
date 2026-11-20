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

//! Package discovery and resolution from already synchronized modules.
//!
//! Discovery produces [`DiscoveredProject`]; package resolution adds validated
//! package relationships to produce [`ResolvedProject`]. Dependency acquisition
//! and script declaration preparation belong to `moonbuild::prepare`.

use crate::{
    discover::{DiscoverError, DiscoverResult, SingleFileSourceKind, discover_for_build},
    pkg_solve::{self, PackageRelations},
    special_cases::self_coverage,
};
use log::{debug, info};
use moonutil::{
    manifest::MoonMod,
    package::Import,
    resolution::{DirSyncResult, ModuleDependencyGraph, ModuleId},
    target::TargetBackend,
    user_log::UserLog,
};
use std::{
    collections::{BTreeMap, HashSet},
    ops::Deref,
    path::Path,
};
use tracing::instrument;

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
    ) -> Result<ResolvedProject, ProjectResolutionError> {
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
                result.map_err(|e| ProjectResolutionError::PackageResolutionError(Box::new(e)))?;
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

/// Settings for package discovery after module dependencies are synchronized.
#[derive(Debug, Default)]
pub struct DiscoveryConfig {
    /// Keep discovery's coverage sources and package resolution's imports aligned.
    pub enable_coverage: bool,
    pub jobs: Option<usize>,
}

/// Failures from package discovery or package relationship resolution.
#[derive(Debug, thiserror::Error)]
pub enum ProjectResolutionError {
    #[error("Failed when discovering packages")]
    DiscoverError(#[from] DiscoverError),

    #[error("Failed to solve package relationship")]
    PackageResolutionError(#[source] Box<pkg_solve::PackageResolutionError>),
}

/// Resolve package relationships for the requested backends from synced dependencies.
#[instrument(skip_all)]
pub fn prepare_synced_project(
    cfg: &DiscoveryConfig,
    synced_dependencies: (ModuleDependencyGraph, DirSyncResult),
    backends: &[TargetBackend],
    user_log: &UserLog,
) -> Result<ResolvedProject, ProjectResolutionError> {
    discover_synced_project(cfg, synced_dependencies, user_log)?
        .resolve_packages(backends, user_log)
}

/// Discover packages from already synced dependencies without solving imports.
#[instrument(skip_all)]
pub fn discover_synced_project(
    cfg: &DiscoveryConfig,
    synced_dependencies: (ModuleDependencyGraph, DirSyncResult),
    user_log: &UserLog,
) -> Result<DiscoveredProject, ProjectResolutionError> {
    let (module_graph, dir_sync_result) = synced_dependencies;

    let mut discover_result =
        discover_for_build(&module_graph, &dir_sync_result, cfg.jobs, user_log)?;
    if cfg.enable_coverage {
        self_coverage::augment_builtin_sources(&module_graph, &mut discover_result);
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

/// Discover the supplied modules and synthesize a package from a prepared
/// single-file declaration. Dependencies must already be available on disk.
/// `source_file` preserves the invoked filename, including symlinks.
#[instrument(skip_all)]
pub fn discover_single_file_project(
    cfg: &DiscoveryConfig,
    synced_dependencies: (ModuleDependencyGraph, DirSyncResult),
    source_file: &Path,
    source_kind: SingleFileSourceKind,
    run_mode: bool,
    imports: Vec<Import>,
    user_log: &UserLog,
) -> Result<DiscoveredProject, ProjectResolutionError> {
    let (module_graph, dir_sync_result) = synced_dependencies;
    // Discover all packages in resolved modules
    let mut discover_result =
        discover_for_build(&module_graph, &dir_sync_result, cfg.jobs, user_log)?;
    // Synthesize the single-file package with only its declared imports.
    crate::discover::synth::build_synth_single_file_package(
        source_file,
        source_kind,
        &module_graph,
        &mut discover_result,
        run_mode,
        imports,
    )?;

    let discovered = DiscoveredProject {
        module_graph,
        module_dirs: dir_sync_result,
        pkg_dirs: discover_result,
        enable_coverage: cfg.enable_coverage,
    };
    Ok(discovered)
}
