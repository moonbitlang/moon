//! High-level abstraction that handles module and package resolving.
//!
//! This module is a relatively straightforward wrapper of relevant functions
//! that needs to be called in order to resolve the build environment.
//! Nevertheless, it should remain pretty useful as it abstracts away
//! intermediate steps and provides a single entry point for resolving the
//! build environment.

use std::path::Path;

use mooncake::pkg::sync::auto_sync;
use moonutil::mooncakes::{
    result::ResolvedEnv, sync::AutoSyncFlags, DirSyncResult, ModuleId, RegistryConfig,
};

use crate::{
    discover::{discover_packages, DiscoverError, DiscoverResult},
    pkg_solve::{self, DepRelationship},
};

/// Represents the overall result of a resolve process.
pub struct ResolveResult {
    /// Module dependency relationship
    pub module_rel: ResolvedEnv,
    /// Module directories
    pub module_dirs: DirSyncResult,
    /// Package directories
    pub pkg_dirs: DiscoverResult,
    /// Package dependency relationship
    pub pkg_rel: DepRelationship,
}

impl ResolveResult {
    pub fn local_modules(&self) -> &[ModuleId] {
        self.module_rel.input_module_ids()
    }
}

#[derive(Debug)]
pub struct ResolveConfig {
    sync_flags: AutoSyncFlags,
    registry_config: RegistryConfig,
}

impl ResolveConfig {
    /// Creates a new `ResolveConfig` with whether to freeze package resolving,
    /// and other flags populated from the environment with a sensible default.
    ///
    /// This method performs IO to load the registry configuration,
    pub fn new_with_load_defaults(frozen: bool) -> Self {
        Self {
            sync_flags: AutoSyncFlags { frozen },
            registry_config: RegistryConfig::load(),
        }
    }

    /// Creates a new `ResolveConfig` with the given flags and registry
    pub fn new(sync_flags: AutoSyncFlags, registry_config: RegistryConfig) -> Self {
        Self {
            sync_flags,
            registry_config,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("Failed to resolve the module dependency graph: {0}")]
    SyncModulesError(anyhow::Error),

    #[error("Failed when discovering packages: {0}")]
    DiscoverError(#[from] DiscoverError),

    #[error("Failed to solve package relationship: {0}")]
    SolveError(#[from] pkg_solve::SolveError),
}

/// Performs the resolving process from a raw working directory, until all of
/// the modules and packages affected are resolved.
pub fn resolve(cfg: &ResolveConfig, source_dir: &Path) -> Result<ResolveResult, ResolveError> {
    let (resolved_env, dir_sync_result) =
        auto_sync(source_dir, &cfg.sync_flags, &cfg.registry_config, false)
            .map_err(ResolveError::SyncModulesError)?;

    let discover_result = discover_packages(&resolved_env, &dir_sync_result)?;

    let dep_relationship = pkg_solve::solve(&resolved_env, &discover_result)?;

    Ok(ResolveResult {
        module_rel: resolved_env,
        module_dirs: dir_sync_result,
        pkg_dirs: discover_result,
        pkg_rel: dep_relationship,
    })
}
