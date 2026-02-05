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
use indexmap::IndexMap;
use log::{debug, info, warn};
use std::str::FromStr;

use mooncake::pkg::sync::{auto_sync, auto_sync_for_single_file_rr};
use moonutil::{
    common::{MOONBITLANG_CORE, MbtMdHeader, TargetBackend, parse_front_matter_config},
    dependency::{SourceDependencyInfo, SourceDependencyInfoJson},
    mooncakes::{
        DirSyncResult, ModuleId, RegistryConfig, result::ResolvedEnv, sync::AutoSyncFlags,
    },
    package::{Import, PkgJSONImport, pkg_json_imports_to_imports},
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

struct FrontMatterImports {
    deps: IndexMap<String, SourceDependencyInfoJson>,
    imports: Vec<Import>,
}

struct FrontMatterConfig {
    deps_to_sync: Option<IndexMap<String, SourceDependencyInfoJson>>,
    package_imports: Option<Vec<Import>>,
    warn_import_all: bool,
}

fn extract_front_matter_config(header: Option<&MbtMdHeader>) -> anyhow::Result<FrontMatterConfig> {
    let mut config = FrontMatterConfig {
        deps_to_sync: None,
        package_imports: None,
        warn_import_all: false,
    };

    let Some(moonbit) = header.and_then(|h| h.moonbit.as_ref()) else {
        return Ok(config);
    };

    match (moonbit.deps.as_ref(), moonbit.import.as_ref()) {
        (Some(_), Some(_)) => {
            anyhow::bail!("moonbit.deps and moonbit.import are mutually exclusive");
        }
        (Some(deps), None) => {
            config.deps_to_sync = Some(deps.clone());
            config.warn_import_all = true;
        }
        (None, Some(_)) => {
            let imports = parse_front_matter_imports(moonbit.import.clone())?;
            config.deps_to_sync = Some(imports.deps);
            config.package_imports = Some(imports.imports);
        }
        (None, None) => {}
    }

    Ok(config)
}

fn parse_front_matter_imports(imports: Option<PkgJSONImport>) -> anyhow::Result<FrontMatterImports> {
    let imports = pkg_json_imports_to_imports(imports);
    let mut deps = IndexMap::new();
    let mut module_versions: IndexMap<String, Option<String>> = IndexMap::new();
    let mut normalized_imports = Vec::with_capacity(imports.len());

    for import in imports {
        let (module, version, package) = split_import_path(import.get_path())?;
        if module == MOONBITLANG_CORE {
            if version.is_some() {
                anyhow::bail!("moonbitlang/core imports must not specify a version");
            }
        }

        let entry = module_versions.entry(module.clone()).or_insert(None);
        if let Some(version) = version {
            match entry {
                Some(existing) if existing.as_str() != version => {
                    anyhow::bail!(
                        "multiple versions specified for module '{module}': '{existing}' and '{version}'"
                    );
                }
                None => {
                    *entry = Some(version.to_string());
                }
                _ => {}
            }
        }

        let normalized_path = match package {
            Some(package) => format!("{module}/{package}"),
            None => module.clone(),
        };
        let normalized_import = match import {
            Import::Simple(_) => Import::Simple(normalized_path),
            Import::Alias {
                path: _,
                alias,
                sub_package,
            } => Import::Alias {
                path: normalized_path,
                alias,
                sub_package,
            },
        };
        normalized_imports.push(normalized_import);
    }

    for (module, version) in module_versions {
        if module == MOONBITLANG_CORE {
            continue;
        }
        let Some(version) = version else {
            anyhow::bail!(
                "module '{module}' must include a version in moonbit.import (e.g. {module}@0.4.40[/package])"
            );
        };
        let version = SourceDependencyInfo::from_str(&version)?;
        deps.insert(module, SourceDependencyInfoJson::from(version));
    }

    Ok(FrontMatterImports {
        deps,
        imports: normalized_imports,
    })
}

fn split_import_path(path: &str) -> anyhow::Result<(String, Option<String>, Option<String>)> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 2 {
        anyhow::bail!(
            "import path '{path}' must be in the form 'username/module@version[/package]'"
        );
    }
    let username = parts[0];
    let module_and_version = parts[1];
    let mut module_parts = module_and_version.splitn(2, '@');
    let module = module_parts.next().unwrap();
    let version = module_parts.next();
    if module.is_empty() {
        anyhow::bail!("import path '{path}' has an empty module name");
    }
    let version = match version {
        Some(v) if v.is_empty() => {
            anyhow::bail!("import path '{path}' has an empty version");
        }
        Some(v) => Some(v.to_string()),
        None => None,
    };
    let package = if parts.len() > 2 {
        let pkg = parts[2..].join("/");
        if pkg.is_empty() {
            anyhow::bail!("import path '{path}' has an empty package path");
        }
        Some(pkg)
    } else {
        None
    };
    Ok((format!("{username}/{module}"), version, package))
}

#[cfg(test)]
mod tests {
    use super::split_import_path;

    #[test]
    fn split_import_path_supports_module_root() {
        let (module, version, package) =
            split_import_path("moonbitlang/async@0.16.5").unwrap();
        assert_eq!(module, "moonbitlang/async");
        assert_eq!(version.as_deref(), Some("0.16.5"));
        assert_eq!(package, None);
    }

    #[test]
    fn split_import_path_supports_module_package() {
        let (module, version, package) =
            split_import_path("moonbitlang/x@0.4.38/stack").unwrap();
        assert_eq!(module, "moonbitlang/x");
        assert_eq!(version.as_deref(), Some("0.4.38"));
        assert_eq!(package.as_deref(), Some("stack"));
    }
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

    let front_matter_config =
        extract_front_matter_config(header.as_ref()).map_err(ResolveError::SingleFileParseError)?;
    if front_matter_config.warn_import_all {
        warn!("moonbit.deps without moonbit.import: importing all packages (legacy behavior)");
    }

    // Sync modules as usual
    let (resolved_env, dir_sync_result) = auto_sync_for_single_file_rr(
        source_dir,
        &cfg.sync_flags,
        front_matter_config.deps_to_sync.as_ref(),
    )
    .map_err(ResolveError::SyncModulesError)?;

    // Discover all packages in resolved modules
    let mut discover_result = discover_packages(&resolved_env, &dir_sync_result)?;

    // Synthesize the single-file package that imports everything from discovered modules
    crate::discover::synth::build_synth_single_file_package(
        &file,
        &resolved_env,
        &mut discover_result,
        run_mode,
        front_matter_config.package_imports,
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
