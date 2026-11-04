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

use std::sync::Arc;

use anyhow::{Context, bail};
use moonutil::{
    manifest::read_module_desc_file_in_dir,
    project::ProjectManifest,
    resolution::{ModuleSource, ModuleSourceKind, ResolvedModule, ResolvedRootModules},
    user_log::UserLog,
};
use semver::Version;

use crate::{
    registry::{Registry, RegistryClient, RegistryVersionInfo},
    resolver::{ModuleResolutionConfig, resolve_modules},
};

/// Independent refresh outcomes for one selected dependency. Keeping failures
/// alongside successful results lets the command report a partial refresh.
pub struct DependencyUpdate {
    pub module: ModuleSource,
    pub status: anyhow::Result<RegistryVersionInfo>,
    pub latest: anyhow::Result<Version>,
}

/// Refresh the selected dependency graph without acquiring sources or changing
/// declarations. Git index synchronization is handled before project discovery.
#[tracing::instrument(skip_all)]
pub fn update(
    project: &ProjectManifest,
    registry: &RegistryClient,
    user_log: &UserLog,
) -> anyhow::Result<Vec<DependencyUpdate>> {
    let module_dirs = match project {
        ProjectManifest::Module(manifest) => {
            vec![manifest.parent().context("module manifest has no parent")?]
        }
        ProjectManifest::Workspace(workspace) => workspace
            .members()
            .iter()
            .map(|path| path.as_path())
            .collect(),
        ProjectManifest::None => bail!("registry refresh requires a module or workspace"),
    };
    let mut roots = ResolvedRootModules::with_key();
    for path in module_dirs {
        let module = Arc::new(read_module_desc_file_in_dir(path)?);
        let source = ModuleSource::from_local_module(&module, path)?;
        roots.insert(ResolvedModule::new(source, module));
    }
    let config = ModuleResolutionConfig {
        registry,
        inject_std: false,
        warn_deprecated: false,
    };
    let graph = resolve_modules(&config, roots, user_log)?;
    let mut selected = graph
        .all_modules_and_id()
        .map(|(_, module)| module)
        .filter(|module| matches!(module.source(), ModuleSourceKind::Registry))
        .collect::<Vec<_>>();
    selected.sort_unstable_by(|a, b| a.name().cmp(b.name()));

    // Resolution produces one selected version per module identity, including
    // shared workspace dependencies. Refresh old selected versions separately
    // from latest: a healthy latest release says nothing about the old one.
    // Git was already synchronized once before project discovery; use that
    // complete snapshot instead of synchronizing again for each dependency.
    selected
        .into_iter()
        .map(|module| {
            let (status, latest) = if registry.uses_http_metadata_for(module.name())? {
                (
                    registry.refresh_version_info(module.name(), module.version(), user_log),
                    registry.query_latest_version(module.name(), user_log),
                )
            } else {
                (
                    registry.cached_version_info(module.name(), module.version()),
                    registry
                        .cached_latest_version(module.name())
                        .and_then(|version| {
                            version.with_context(|| {
                                format!("module {} has no versions", module.name())
                            })
                        }),
                )
            };
            Ok(DependencyUpdate {
                status: status
                    .and_then(|info| info.with_context(|| format!("release {module} is missing"))),
                latest,
                module: module.clone(),
            })
        })
        .collect()
}
