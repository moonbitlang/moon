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
//! Normal project resolution is split into an explicit dependency sync step and
//! a package discovery/solve step. This keeps dependency-directory mutation
//! visible to command adapters before RR consumes the synced dependencies as
//! input.

use std::path::Path;

use anyhow::Context;
use indexmap::IndexMap;
use log::{debug, info};
use std::str::FromStr;

use mooncake::{
    pkg::sync::{SyncOutputOptions, auto_sync, auto_sync_for_single_file_rr},
    registry::path as registry_path,
};
use moonutil::{
    cli_support::AutoSyncFlags,
    constants::{MBTI_USER_WRITTEN, MOONBITLANG_CORE},
    dependency::SourceDependencyInfo,
    front_matter::{MbtMdHeader, parse_front_matter_config},
    manifest::MoonMod,
    package::{Import, PkgJSONImport, pkg_json_imports_to_imports},
    project::{PackageDirs, WorkspaceEnv},
    resolution::{DirSyncResult, ModuleId, ResolvedEnv},
    target::TargetBackend,
    user_log::UserLog,
};
use tracing::instrument;

use crate::mbtx::{parse_mbtx_imports, prepare_single_file_for_compile};

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
    /// Registry sources whose immutable archive checksums were verified while
    /// preparing the shared dependency cache.
    pub prepared_sources: mooncake::prepared_source::PreparedSourceMap,
    /// Package directories
    pub pkg_dirs: DiscoverResult,
    /// Package dependency relationship
    pub pkg_rel: DepRelationship,
}

impl ResolveOutput {
    /// Returns the input/root modules of the current resolve.
    ///
    /// This is a role in the current resolution graph, not a check of
    /// `ModuleSourceKind::Local`.
    pub fn local_modules(&self) -> &[ModuleId] {
        self.module_rel.input_module_ids()
    }

    pub fn module_info(&self, id: ModuleId) -> &MoonMod {
        self.pkg_dirs.module_info(id)
    }
}

#[derive(Debug)]
pub struct ResolveConfig {
    sync_flags: AutoSyncFlags,
    sync_output: SyncOutputOptions,
    no_std: bool,
    /// Whether direct bin-deps of the input modules participate in resolution
    /// and are installed during dependency sync.
    include_bin_deps: bool,
    /// Gate coverage injection in pkg_solve
    pub enable_coverage: bool,
    workspace_env: WorkspaceEnv,
}

struct FrontMatterImports {
    deps: IndexMap<String, SourceDependencyInfo>,
    imports: Vec<Import>,
}

struct FrontMatterConfig {
    deps_to_sync: Option<IndexMap<String, SourceDependencyInfo>>,
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

fn warn_virtual_mbti_deprecations(packages: &DiscoverResult, user_log: &UserLog) {
    for (_pkg_id, pkg) in packages.all_packages(false) {
        if let Some(virtual_mbti) = &pkg.virtual_mbti
            && virtual_mbti.file_name().and_then(|name| name.to_str()) != Some(MBTI_USER_WRITTEN)
        {
            user_log.warn(format!(
                "Using package name in MBTI file is deprecated. Please rename {} to {}",
                virtual_mbti.display(),
                MBTI_USER_WRITTEN
            ));
        }
    }
}

fn parse_front_matter_imports(
    imports: Option<PkgJSONImport>,
) -> anyhow::Result<FrontMatterImports> {
    let imports = pkg_json_imports_to_imports(imports);
    let mut deps = IndexMap::new();
    let mut module_versions: IndexMap<String, Option<String>> = IndexMap::new();
    let mut normalized_imports = Vec::with_capacity(imports.len());

    for import in imports {
        let (module, version, package) = split_import_path(import.get_path())?;
        if module == MOONBITLANG_CORE && version.is_some() {
            anyhow::bail!("moonbitlang/core imports must not specify a version");
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
        deps.insert(module, version);
    }

    Ok(FrontMatterImports {
        deps,
        imports: normalized_imports,
    })
}

fn split_import_path(path: &str) -> anyhow::Result<(String, Option<String>, Option<String>)> {
    let parsed = registry_path::parse_front_matter_import_path(path)?;
    Ok((parsed.module, parsed.version, parsed.package))
}

#[cfg(test)]
mod tests {
    use super::split_import_path;

    #[test]
    fn split_import_path_supports_module_root() {
        let (module, version, package) =
            split_import_path("moonbitlang/async@0.16.5").expect("module-root import should parse");
        assert_eq!(module, "moonbitlang/async");
        assert_eq!(version.as_deref(), Some("0.16.5"));
        assert_eq!(package, None);
    }

    #[test]
    fn split_import_path_supports_module_package() {
        let (module, version, package) =
            split_import_path("moonbitlang/x@0.4.38/stack").expect("module import should parse");
        assert_eq!(module, "moonbitlang/x");
        assert_eq!(version.as_deref(), Some("0.4.38"));
        assert_eq!(package.as_deref(), Some("stack"));
    }

    #[test]
    fn split_import_path_rejects_package_version_suffix() {
        assert!(split_import_path("moonbitlang/x/stack@0.4.38").is_err());
    }
}

impl ResolveConfig {
    /// Creates a new `ResolveConfig` with whether to freeze package resolving,
    /// and other flags populated with sensible defaults.
    pub fn new_with_load_defaults(
        frozen: bool,
        no_std: bool,
        enable_coverage: bool,
        workspace_env: WorkspaceEnv,
    ) -> Self {
        Self {
            sync_flags: AutoSyncFlags { frozen },
            sync_output: SyncOutputOptions::default(),
            no_std,
            include_bin_deps: true,
            enable_coverage,
            workspace_env,
        }
    }

    /// Creates a new `ResolveConfig` with the given sync and build flags.
    pub fn new(
        sync_flags: AutoSyncFlags,
        no_std: bool,
        enable_coverage: bool,
        workspace_env: WorkspaceEnv,
    ) -> Self {
        Self {
            sync_flags,
            sync_output: SyncOutputOptions::default(),
            no_std,
            include_bin_deps: true,
            enable_coverage,
            workspace_env,
        }
    }

    pub fn with_quiet_sync(mut self, quiet_sync: bool) -> Self {
        self.sync_output = self.sync_output.with_quiet(quiet_sync);
        self
    }

    pub fn with_sync_output(mut self, sync_output: SyncOutputOptions) -> Self {
        self.sync_output = sync_output;
        self
    }

    pub fn without_bin_deps(mut self) -> Self {
        self.include_bin_deps = false;
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("Failed to resolve the module dependency graph")]
    SyncModulesError(#[source] anyhow::Error),

    #[error("Failed when discovering packages")]
    DiscoverError(#[from] DiscoverError),

    #[error("Failed to solve package relationship")]
    SolveError(#[source] Box<pkg_solve::SolveError>),

    #[error("Failed to parse single file front matter configuration")]
    SingleFileParseError(#[source] anyhow::Error),
}

/// Performs the resolving process from a raw working directory, until all of
/// modules and package directories are ready for package discovery.
#[instrument(skip_all)]
pub fn sync_dependencies(
    cfg: &ResolveConfig,
    dirs: &PackageDirs,
) -> Result<(ResolvedEnv, DirSyncResult), ResolveError> {
    info!(
        "Starting dependency sync for source directory: {}",
        dirs.source_dir.display()
    );
    debug!("Resolve config: sync_flags={:?}", cfg.sync_flags);

    let (resolved_env, dir_sync_result, _) = auto_sync(
        dirs,
        &cfg.sync_flags,
        cfg.sync_output,
        cfg.no_std,
        cfg.workspace_env.clone(),
        cfg.include_bin_deps,
    )
    .map_err(ResolveError::SyncModulesError)?;
    info!("Module dependency resolution completed successfully");
    debug!("Resolved {} modules", resolved_env.module_count());

    Ok((resolved_env, dir_sync_result))
}

/// Resolves packages and package relationships from already synced dependencies.
#[instrument(skip_all)]
pub fn resolve_synced_project(
    cfg: &ResolveConfig,
    synced_dependencies: (ResolvedEnv, DirSyncResult),
    user_log: &UserLog,
) -> Result<ResolveOutput, ResolveError> {
    let (resolved_env, dir_sync_result) = synced_dependencies;

    let mut discover_result = discover_packages(&resolved_env, &dir_sync_result)?;
    let main_is_core = {
        let ids = resolved_env.input_module_ids();
        ids.len() == 1 && *resolved_env.module_source(ids[0]).name() == CORE_MODULE_TUPLE
    };
    if cfg.enable_coverage && main_is_core {
        // Gate coverage bundling (coverage -> builtin) behind both flag and main-module check
        inject_core_coverage_into_builtin(&resolved_env, &mut discover_result);
    }

    info!(
        "Package discovery completed, found {} packages",
        discover_result.package_count()
    );

    warn_virtual_mbti_deprecations(&discover_result, user_log);
    let dep_relationship = pkg_solve::solve(
        &resolved_env,
        &discover_result,
        cfg.enable_coverage,
        user_log,
    )
    .map_err(|source| ResolveError::SolveError(Box::new(source)))?;

    info!("Package dependency resolution completed successfully");
    debug!(
        "Package dependency graph has {} nodes",
        dep_relationship.dep_graph.node_count()
    );

    Ok(ResolveOutput {
        module_rel: resolved_env,
        module_dirs: dir_sync_result,
        prepared_sources: Default::default(),
        pkg_dirs: discover_result,
        pkg_rel: dep_relationship,
    })
}

/// Performs the resolving process for a single file project. Will try to
/// synthesize a minimal MoonBit project around the given file.
#[instrument(skip_all, fields(run_mode = run_mode))]
pub fn resolve_single_file_project(
    cfg: &ResolveConfig,
    dirs: &PackageDirs,
    source_file: &Path,
    run_mode: bool,
    user_log: &UserLog,
) -> Result<(ResolveOutput, Option<TargetBackend>), ResolveError> {
    let is_mbtx = source_file.extension().is_some_and(|ext| ext == "mbtx");
    let (header, front_matter_config, compile_input_file) = if is_mbtx {
        let imports =
            parse_mbtx_imports(source_file).map_err(ResolveError::SingleFileParseError)?;
        let mut config = FrontMatterConfig {
            deps_to_sync: None,
            package_imports: None,
            warn_import_all: false,
        };
        if !imports.deps.is_empty() || !imports.imports.is_empty() {
            config.deps_to_sync = Some(imports.deps);
            config.package_imports = Some(imports.imports);
        }
        // Generate a temporary .mbt file under target_dir,
        // because moonc doesn't support import declarations in source files yet.
        let compile_input_file = prepare_single_file_for_compile(source_file, &dirs.target_dir)
            .map_err(ResolveError::SingleFileParseError)?;
        (None, config, compile_input_file)
    } else {
        let header =
            parse_front_matter_config(source_file).map_err(ResolveError::SingleFileParseError)?;
        let config = extract_front_matter_config(header.as_ref())
            .map_err(ResolveError::SingleFileParseError)?;
        (header, config, source_file.to_path_buf())
    };

    let backend = header
        .as_ref()
        .and_then(|h| h.moonbit.as_ref())
        .and_then(|mb| mb.backend.as_ref())
        .map(|b| TargetBackend::str_to_backend(b))
        // Error handling
        .transpose()
        .context("Unable to parse target backend from front matter")
        .map_err(ResolveError::SingleFileParseError)?;

    if front_matter_config.warn_import_all {
        user_log.warn(
            "moonbit.deps without moonbit.import: importing all packages (legacy behavior). \
Use moonbit.import with 'username/module@version[/package]' entries to opt in to explicit imports.",
        );
    }

    // Sync modules as usual
    let (resolved_env, prepared_sources) = auto_sync_for_single_file_rr(
        dirs,
        &cfg.sync_flags,
        front_matter_config.deps_to_sync.as_ref(),
        cfg.sync_output,
    )
    .map_err(ResolveError::SyncModulesError)?;
    let (dir_sync_result, prepared_sources) = prepared_sources.into_parts();
    // Discover all packages in resolved modules
    let mut discover_result = discover_packages(&resolved_env, &dir_sync_result)?;
    warn_virtual_mbti_deprecations(&discover_result, user_log);

    // Synthesize the single-file package that imports everything from discovered modules
    crate::discover::synth::build_synth_single_file_package(
        &compile_input_file,
        &resolved_env,
        &mut discover_result,
        run_mode,
        front_matter_config.package_imports,
    )?;

    // Solve package dependency relationship
    let dep_relationship = pkg_solve::solve(
        &resolved_env,
        &discover_result,
        cfg.enable_coverage,
        user_log,
    )
    .map_err(|source| ResolveError::SolveError(Box::new(source)))?;

    let res = ResolveOutput {
        module_rel: resolved_env,
        module_dirs: dir_sync_result,
        prepared_sources,
        pkg_dirs: discover_result,
        pkg_rel: dep_relationship,
    };
    Ok((res, backend))
}
