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

use std::{rc::Rc, sync::Arc};

use anyhow::{Context, bail};
use moonutil::{
    manifest::read_module_desc_file_in_dir,
    project::ProjectManifest,
    resolution::{ModuleSource, ModuleSourceKind, ResolvedModule, ResolvedRootModules},
    user_log::UserLog,
};
use semver::Version;
use tokio::task::JoinSet;
use tracing::Instrument;

use crate::{
    registry::{RegistryClient, RegistryVersionInfo},
    resolver::{ModuleResolutionConfig, MvsSolver, resolve_modules_with_solver},
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
/// The caller owns the Tokio runtime and LocalSet for resolution and refresh.
#[tracing::instrument(skip_all)]
pub async fn update(
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
    let graph = resolve_modules_with_solver(&config, &mut MvsSolver, roots, user_log).await?;
    let selected = graph
        .all_modules_and_id()
        .map(|(_, module)| module)
        .filter(|module| matches!(module.source(), ModuleSourceKind::Registry));

    // Resolution produces one selected version per module identity, including
    // shared workspace dependencies. Refresh old selected versions separately
    // from latest: a healthy latest release says nothing about the old one.
    let reader = Rc::new(registry.http_version_reader()?);
    let mut refreshes = JoinSet::new();
    for module in selected {
        let module = module.clone();
        let reader = Rc::clone(&reader);
        let span = tracing::debug_span!("refresh_dependency", module = %module);
        refreshes.spawn_local(
            async move {
                let status = reader
                    .refresh_version_info(module.name(), module.version())
                    .await;
                // Attempt latest even if refreshing the selected version failed.
                let latest = reader.query_latest_version(module.name()).await;
                DependencyUpdate {
                    module,
                    status,
                    latest,
                }
            }
            .instrument(span),
        );
    }
    let mut updates = Vec::new();
    while let Some(update) = refreshes.join_next().await {
        updates.push(update.context("registry refresh task failed")?);
    }
    updates.sort_unstable_by(|a, b| a.module.name().cmp(b.module.name()));
    Ok(updates)
}
