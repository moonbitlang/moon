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

//! High-level abstraction that handles module and package resolving.
//!
//! This module is a relatively straightforward wrapper of relevant functions
//! that needs to be called in order to resolve the build environment.
//! Nevertheless, it should remain pretty useful as it abstracts away
//! intermediate steps and provides a single entry point for resolving the
//! build environment.

use std::path::Path;

use anyhow::Context;
use log::{debug, info};

use mooncake::pkg::sync::{auto_sync, auto_sync_for_single_file_rr};
use moonutil::{
    common::{TargetBackend, parse_front_matter_config},
    mooncakes::{
        DirSyncResult, ModuleId, RegistryConfig, result::ResolvedEnv, sync::AutoSyncFlags,
    },
    package::pkg_json_imports_to_imports,
};
use tracing::instrument;

use crate::discover::special_case::inject_core_coverage_into_builtin;
use crate::special_cases::CORE_MODULE_TUPLE;
use crate::{
    discover::{DiscoverError, DiscoverResult, discover_packages},
    pkg_solve::{self, DepRelationship},
};

/// Represents the overall result of a resolve process.
#[derive(Debug, Clone)]
pub struct ResolveOutput {
    /// Module dependency relationship
    pub module_rel: ResolvedEnv,
    /// Module directories
    pub module_dirs: DirSyncResult,
    /// Package directories
    pub pkg_dirs: DiscoverResult,
    /// Package dependency relationship
    pub pkg_rel: DepRelationship,
}

impl ResolveOutput {
    pub fn local_modules(&self) -> &[ModuleId] {
        self.module_rel.input_module_ids()
    }
}

#[derive(Debug)]
pub struct ResolveConfig {
    sync_flags: AutoSyncFlags,
    registry_config: RegistryConfig,
    no_std: bool,
    /// Gate coverage injection in pkg_solve
    pub enable_coverage: bool,
}

impl ResolveConfig {
    /// Creates a new `ResolveConfig` with whether to freeze package resolving,
    /// and other flags populated from the environment with a sensible default.
    ///
    /// This method performs IO to load the registry configuration,
    pub fn new_with_load_defaults(frozen: bool, no_std: bool, enable_coverage: bool) -> Self {
        Self {
            sync_flags: AutoSyncFlags { frozen },
            registry_config: RegistryConfig::load(),
            no_std,
            enable_coverage,
        }
    }

    /// Creates a new `ResolveConfig` with the given flags and registry
    pub fn new(
        sync_flags: AutoSyncFlags,
        registry_config: RegistryConfig,
        no_std: bool,
        enable_coverage: bool,
    ) -> Self {
        Self {
            sync_flags,
            registry_config,
            no_std,
            enable_coverage,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("Failed to resolve the module dependency graph")]
    SyncModulesError(#[source] anyhow::Error),

    #[error("Failed when discovering packages")]
    DiscoverError(#[from] DiscoverError),

    #[error("Failed to solve package relationship")]
    SolveError(#[from] pkg_solve::SolveError),

    #[error("Failed to parse single file front matter configuration")]
    SingleFileParseError(#[source] anyhow::Error),
}

/// Performs the resolving process from a raw working directory, until all of
/// the modules and packages affected are resolved.
#[instrument(skip_all)]
pub fn resolve(cfg: &ResolveConfig, source_dir: &Path) -> Result<ResolveOutput, ResolveError> {
    info!(
        "Starting resolve process for source directory: {}",
        source_dir.display()
    );
    debug!("Resolve config: sync_flags={:?}", cfg.sync_flags);

    let (resolved_env, dir_sync_result) = auto_sync(
        source_dir,
        &cfg.sync_flags,
        &cfg.registry_config,
        false,
        cfg.no_std,
    )
    .map_err(ResolveError::SyncModulesError)?;

    info!("Module dependency resolution completed successfully");
    debug!("Resolved {} modules", resolved_env.module_count());

    let mut discover_result = discover_packages(&resolved_env, &dir_sync_result)?;
    let main_is_core = {
        let ids = resolved_env.input_module_ids();
        ids.len() == 1 && *resolved_env.mod_name_from_id(ids[0]).name() == CORE_MODULE_TUPLE
    };
    if cfg.enable_coverage && main_is_core {
        // Gate coverage bundling (coverage -> builtin) behind both flag and main-module check
        inject_core_coverage_into_builtin(&resolved_env, &mut discover_result)?;
    }

    info!(
        "Package discovery completed, found {} packages",
        discover_result.package_count()
    );

    let dep_relationship = pkg_solve::solve(&resolved_env, &discover_result, cfg.enable_coverage)?;

    info!("Package dependency resolution completed successfully");
    debug!(
        "Package dependency graph has {} nodes",
        dep_relationship.dep_graph.node_count()
    );

    Ok(ResolveOutput {
        module_rel: resolved_env,
        module_dirs: dir_sync_result,
        pkg_dirs: discover_result,
        pkg_rel: dep_relationship,
    })
}

/// Performs the resolving process for a single file project. Will try to
/// synthesize a minimal MoonBit project around the given file.
pub fn resolve_single_file_project(
    cfg: &ResolveConfig,
    file: &Path,
    run_mode: bool,
) -> Result<(ResolveOutput, Option<TargetBackend>), ResolveError> {
    // Canonicalize input and parse optional front matter
    let file = dunce::canonicalize(file)
        .context("Failed to resolve the file path")
        .map_err(ResolveError::SingleFileParseError)?;
    let header = parse_front_matter_config(&file).map_err(ResolveError::SingleFileParseError)?;

    let backend = header
        .as_ref()
        .and_then(|h| h.moonbit.as_ref())
        .and_then(|mb| mb.backend.as_ref())
        .map(|b| TargetBackend::str_to_backend(b))
        // Error handling
        .transpose()
        .context("Unable to parse target backend from front matter")
        .map_err(ResolveError::SingleFileParseError)?;

    let source_dir = file.parent().expect("File must have a parent directory");
    let front_matter_imports = header
        .as_ref()
        .and_then(|h| h.moonbit.as_ref())
        .and_then(|mb| mb.import.clone())
        .map(|imports| pkg_json_imports_to_imports(Some(imports)));

    // Sync modules as usual
    let (resolved_env, dir_sync_result) =
        auto_sync_for_single_file_rr(source_dir, &cfg.sync_flags, header.as_ref())
            .map_err(ResolveError::SyncModulesError)?;

    // Discover all packages in resolved modules
    let mut discover_result = discover_packages(&resolved_env, &dir_sync_result)?;

    // Synthesize the single-file package that imports everything from discovered modules
    crate::discover::synth::build_synth_single_file_package(
        &file,
        &resolved_env,
        &mut discover_result,
        run_mode,
        front_matter_imports,
    )?;

    // Solve package dependency relationship
    let dep_relationship = pkg_solve::solve(&resolved_env, &discover_result, cfg.enable_coverage)?;

    let res = ResolveOutput {
        module_rel: resolved_env,
        module_dirs: dir_sync_result,
        pkg_dirs: discover_result,
        pkg_rel: dep_relationship,
    };
    Ok((res, backend))
}
