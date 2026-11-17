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

//! Interpret script declarations and synchronize their module dependencies.

mod mbtx;

use super::{ProjectPreparationConfig, ProjectPreparationError};
use anyhow::Context;
use indexmap::IndexMap;
use mbtx::parse_mbtx_imports;
use moonbuild_rupes_recta::{DiscoveredProject, discover::SingleFileSourceKind};
use mooncake::{pkg::sync::auto_sync_for_single_file_rr, registry::path as registry_path};
use moonutil::{
    constants::MOONBITLANG_CORE,
    dependency::SourceDependencyInfo,
    front_matter::{MbtMdHeader, parse_front_matter_config},
    package::{Import, PkgJSONImport, pkg_json_imports_to_imports},
    project::PackageDirs,
    target::TargetBackend,
    user_log::UserLog,
};
use std::{path::Path, str::FromStr};
use tracing::instrument;

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
    if source_kind == SingleFileSourceKind::Mbt {
        user_log.warn(
            "Standalone `.mbt` files are deprecated. Use `.mbtx` for scripts and declare imports in front matter.",
        );
    }
    let (header, front_matter_config) = if source_kind == SingleFileSourceKind::Mbtx {
        let imports = parse_mbtx_imports(source_file, user_log)
            .map_err(ProjectPreparationError::SingleFileParseError)?;
        let config = FrontMatterConfig {
            deps_to_sync: Some(imports.deps),
            package_imports: imports.imports,
        };
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

    let (module_graph, dir_sync_result) = auto_sync_for_single_file_rr(
        dirs,
        &cfg.sync_flags,
        front_matter_config.deps_to_sync.as_ref(),
        cfg.sync_output,
        &cfg.dependency_source_cache,
        cfg.discovery.jobs,
        user_log,
    )
    .map_err(ProjectPreparationError::SyncModulesError)?;
    let discovered = moonbuild_rupes_recta::resolve::discover_single_file_project(
        &cfg.discovery,
        (module_graph, dir_sync_result),
        source_file,
        source_kind,
        run_mode,
        front_matter_config.package_imports,
        user_log,
    )?;
    Ok((discovered, backend))
}

struct FrontMatterImports {
    deps: IndexMap<String, SourceDependencyInfo>,
    imports: Vec<Import>,
}

#[derive(Default)]
struct FrontMatterConfig {
    deps_to_sync: Option<IndexMap<String, SourceDependencyInfo>>,
    package_imports: Vec<Import>,
}

fn extract_front_matter_config(header: Option<&MbtMdHeader>) -> anyhow::Result<FrontMatterConfig> {
    let mut config = FrontMatterConfig::default();

    let Some(moonbit) = header.and_then(|h| h.moonbit.as_ref()) else {
        return Ok(config);
    };

    match (moonbit.deps.as_ref(), moonbit.import.as_ref()) {
        (Some(_), Some(_)) => {
            anyhow::bail!("moonbit.deps and moonbit.import are mutually exclusive");
        }
        (Some(deps), None) => {
            config.deps_to_sync = Some(deps.clone());
        }
        (None, Some(_)) => {
            let imports = parse_front_matter_imports(moonbit.import.clone())?;
            config.deps_to_sync = Some(imports.deps);
            config.package_imports = imports.imports;
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
