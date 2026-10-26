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

use std::{collections::HashMap, sync::Arc};

use anyhow::bail;
use moonutil::{
    manifest::read_module_desc_file_in_dir,
    project::{PackageDirs, ProjectManifest},
    resolution::{ModuleSource, ModuleSourceKind, ResolvedModule, ResolvedRootModules},
    user_log::UserLog,
};
use semver::Version;

use crate::{
    registry::RegistryClient,
    resolver::{
        ModuleResolutionConfig, resolve_modules_without_deprecation_warnings,
        warn_deprecated_dependencies,
    },
};

pub struct AvailableUpdate {
    pub selected: ModuleSource,
    pub latest: Version,
}

/// Successful writes survive partial failures. The CLI renders available updates
/// before returning a nonzero exit code for any incomplete refreshes.
pub struct RegistryUpdateReport {
    pub updates: Vec<AvailableUpdate>,
    pub failures: Vec<String>,
}

/// Resolve metadata only: do not install sources, mutate manifests or run hooks.
pub fn update(
    dirs: &PackageDirs,
    registry: &RegistryClient,
    user_log: &UserLog,
) -> anyhow::Result<RegistryUpdateReport> {
    let module_dirs = match &dirs.project_manifest {
        ProjectManifest::Module(path) => vec![
            path.parent()
                .expect("module manifest has a parent")
                .to_path_buf(),
        ],
        ProjectManifest::Workspace(workspace) => workspace.members().to_vec(),
        ProjectManifest::None => bail!("moon update in API mode requires a project"),
    };
    let mut roots = ResolvedRootModules::with_key();
    for dir in module_dirs {
        let module = Arc::new(read_module_desc_file_in_dir(&dir)?);
        let source = ModuleSource::from_local_module(&module, &dir)?;
        roots.insert(ResolvedModule::new(source, module));
    }
    let config = ModuleResolutionConfig {
        registry,
        inject_std: false,
    };
    let graph = resolve_modules_without_deprecation_warnings(&config, roots, user_log)?;
    let mut selected = graph
        .all_modules_and_id()
        .map(|(_, source)| source)
        .filter(|source| matches!(source.source(), ModuleSourceKind::Registry))
        .collect::<Vec<_>>();
    selected.sort_unstable_by(|a, b| a.name().cmp(b.name()).then(a.version().cmp(b.version())));

    let mut report = RegistryUpdateReport {
        updates: Vec::new(),
        failures: Vec::new(),
    };
    let mut latest = HashMap::new();
    for source in selected {
        if let Err(error) = registry.refresh_version_info(source.name(), source.version()) {
            report
                .failures
                .push(format!("status for {source}: {error:#}"));
        }
        let latest_version = latest
            .entry(source.name().clone())
            .or_insert_with(|| match registry.refresh_latest_version(source.name()) {
                Ok(version) => Some(version),
                Err(error) => {
                    report
                        .failures
                        .push(format!("latest for {}: {error:#}", source.name()));
                    None
                }
            });
        if let Some(version) = latest_version.as_ref()
            && version > source.version()
        {
            report.updates.push(AvailableUpdate {
                selected: source.clone(),
                latest: version.clone(),
            });
        }
    }
    warn_deprecated_dependencies(&graph, registry, user_log);
    Ok(report)
}
