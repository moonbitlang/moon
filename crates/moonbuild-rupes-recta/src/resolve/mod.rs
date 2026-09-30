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

use anyhow::Context;
use indexmap::IndexMap;
use log::{debug, info};
use std::str::FromStr;

use mooncake::{
    pkg::sync::{SyncOutputOptions, auto_sync, auto_sync_for_single_file_rr},
    registry::path as registry_path,
};
use moonutil::{
    cache::CacheRoot,
    cli_support::AutoSyncFlags,
    constants::MOONBITLANG_CORE,
    dependency::SourceDependencyInfo,
    front_matter::{MbtMdHeader, parse_front_matter_config},
    manifest::MoonMod,
    package::{Import, PkgJSONImport, pkg_json_imports_to_imports},
    project::{PackageDirs, WorkspaceEnv},
    resolution::{DirSyncResult, ModuleDependencyGraph, ModuleId},
    target::TargetBackend,
    user_log::UserLog,
};
use tracing::instrument;

use crate::mbtx::parse_mbtx_imports;

use crate::discover::special_case::inject_core_coverage_into_builtin;
use crate::special_cases::CORE_MODULE_TUPLE;
use crate::{
    discover::{DiscoverError, DiscoverResult, SingleFileSourceKind, discover_packages},
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
    /// Whether direct bin-deps of the input modules participate in module resolution
    /// and are installed during module sync.
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
                import_all,
                targets,
            } => Import::Alias {
                path: normalized_path,
                alias,
                sub_package,
                import_all,
                targets,
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
    Ok((
        parsed.module.to_string(),
        parsed.version,
        (!parsed.package.is_empty()).then_some(parsed.package),
    ))
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
            include_bin_deps: true,
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
            include_bin_deps: true,
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

    pub fn without_bin_deps(mut self) -> Self {
        self.include_bin_deps = false;
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

    #[error("Failed to parse single file front matter configuration")]
    SingleFileParseError(#[source] anyhow::Error),
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
        cfg.include_bin_deps,
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

/// Discover a single-file project and read its preferred backend.
/// The caller chooses explicit/header/default backends before package resolution.
/// `source_file` must be the absolute invoked path from
/// `SingleFilePackageDirs::input_path`, preserving a file symlink's own filename.
#[instrument(skip_all, fields(run_mode = run_mode))]
pub fn discover_single_file_project(
    cfg: &ProjectPreparationConfig,
    dirs: &PackageDirs,
    source_file: &Path,
    run_mode: bool,
    user_log: &UserLog,
) -> Result<(DiscoveredProject, Option<TargetBackend>), ProjectPreparationError> {
    let source_kind = if source_file.extension().is_some_and(|ext| ext == "mbtx") {
        SingleFileSourceKind::Mbtx
    } else if source_file.extension().is_some_and(|ext| ext == "md") {
        SingleFileSourceKind::MbtMd
    } else {
        SingleFileSourceKind::Mbt
    };
    let (header, front_matter_config) = if source_kind == SingleFileSourceKind::Mbtx {
        let imports = parse_mbtx_imports(source_file)
            .map_err(ProjectPreparationError::SingleFileParseError)?;
        let mut config = FrontMatterConfig {
            deps_to_sync: None,
            package_imports: None,
            warn_import_all: false,
        };
        if !imports.deps.is_empty() || !imports.imports.is_empty() {
            config.deps_to_sync = Some(imports.deps);
            config.package_imports = Some(imports.imports);
        }
        (None, config)
    } else {
        let header = parse_front_matter_config(source_file)
            .map_err(ProjectPreparationError::SingleFileParseError)?;
        let config = extract_front_matter_config(header.as_ref())
            .map_err(ProjectPreparationError::SingleFileParseError)?;
        (header, config)
    };

    let backend = header
        .as_ref()
        .and_then(|h| h.moonbit.as_ref())
        .and_then(|mb| mb.backend.as_ref())
        .map(|b| TargetBackend::str_to_backend(b))
        // Error handling
        .transpose()
        .context("Unable to parse target backend from front matter")
        .map_err(ProjectPreparationError::SingleFileParseError)?;

    if front_matter_config.warn_import_all {
        user_log.warn(
            "moonbit.deps without moonbit.import: importing all packages (legacy behavior). \
Use moonbit.import with 'username/module@version[/package]' entries to opt in to explicit imports.",
        );
    }

    let (module_graph, dir_sync_result) = auto_sync_for_single_file_rr(
        dirs,
        &cfg.sync_flags,
        front_matter_config.deps_to_sync.as_ref(),
        cfg.sync_output,
        &cfg.dependency_source_cache,
        user_log,
    )
    .map_err(ProjectPreparationError::SyncModulesError)?;
    // Discover all packages in resolved modules
    let mut discover_result = discover_packages(&module_graph, &dir_sync_result, user_log)?;
    // Synthesize the single-file package that imports everything from discovered modules
    crate::discover::synth::build_synth_single_file_package(
        source_file,
        source_kind,
        &module_graph,
        &mut discover_result,
        run_mode,
        front_matter_config.package_imports,
    )?;

    let discovered = DiscoveredProject {
        module_graph,
        module_dirs: dir_sync_result,
        pkg_dirs: discover_result,
        enable_coverage: cfg.enable_coverage,
    };
    Ok((discovered, backend))
}
