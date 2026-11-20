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

//! Compile persistent standalone sources without constructing CLI commands.

use super::{BuildSession, MetadataOutput, prepare_backend, prepare_config};
use crate::{
    execution::{BuildConfig, OutputStyle},
    prepare::{ProjectPreparationConfig, discover_single_file_project},
};
use anyhow::Context;
use moonbuild_rupes_recta::{
    RunMode,
    build_plan::ArtifactKey,
    intent::UserIntent,
    model::{DebugInfoRequest, DebugSymbols},
};
use mooncake::pkg::sync::SyncOutputOptions;
use moonutil::{
    cache::{CacheKind, resolve_cache_root},
    cond_expr::OptLevel,
    front_matter::MbtxFrontMatter,
    project::SingleFilePackageDirs,
    target::TargetBackend,
    user_log::{UserLog, user_log_level},
};
use std::path::PathBuf;

/// A compiled Wasm artifact and its optional embedded runtime-policy source.
/// The build session has ended before this result is returned.
pub struct StandaloneWasm {
    pub executable: PathBuf,
    /// Original `.mbtx` source when it declares permissions. Runtime policy
    /// roots remain relative to that source's directory.
    // TODO(moonrun-execution-request): Replace this source-file handoff once
    // Moonrun exposes a directly usable daemon/IPC execution entry accepting
    // permission declarations and their source directory with the run inputs.
    // Moonrun should select and resolve policy without rereading the source;
    // compilation results should then contain only build artifacts.
    pub policy_source: Option<PathBuf>,
}

/// Build a persistent standalone source as linear-memory Wasm.
#[tracing::instrument(skip_all)]
pub fn build_standalone_wasm(
    dirs: SingleFilePackageDirs,
    frozen: bool,
    verbose: bool,
) -> anyhow::Result<StandaloneWasm> {
    let SingleFilePackageDirs {
        package_dirs: dirs,
        input_path,
        ..
    } = dirs;
    let policy_source = if input_path.extension().is_some_and(|ext| ext == "mbtx")
        && MbtxFrontMatter::from_file(&input_path)?
            .permission
            .is_some()
    {
        Some(input_path.clone())
    } else {
        None
    };
    let user_log = UserLog::new(user_log_level(verbose, !verbose));
    std::fs::create_dir_all(&dirs.target_dir).context("failed to create target directory")?;
    let preparation = ProjectPreparationConfig::new_with_load_defaults(frozen, false)
        .with_sync_output(SyncOutputOptions { quiet: !verbose })
        .with_dependency_source_cache(
            resolve_cache_root(CacheKind::DependencySources)
                .context("Failed to resolve the module dependency graph")?,
        );
    let (discovered, _) =
        discover_single_file_project(&preparation, &dirs, &input_path, true, &user_log)?;
    let resolved = discovered.resolve_packages(&[TargetBackend::Wasm], &user_log)?;
    let session = BuildSession::new(&dirs.target_dir, false, &user_log)?;
    let mut config = prepare_config(
        &resolved,
        &dirs.target_dir,
        prepare_backend(
            TargetBackend::Wasm,
            false,
            moonutil::features::FeatureGate::default().wasi_link,
        )?,
        OptLevel::Debug,
        RunMode::Run,
        DebugInfoRequest {
            symbols: DebugSymbols::Full,
            runtime_backtrace: false,
        },
    );
    config.moonc_output_json = true;
    let package = resolved
        .local_modules()
        .iter()
        .flat_map(|module| {
            resolved
                .pkg_dirs
                .packages_for_module(*module)
                .into_iter()
                .flat_map(|packages| packages.values().copied())
        })
        .next()
        .expect("Single-file project must synthesize exactly one package");
    let (meta, input) = session.plan(
        config,
        vec![UserIntent::Run(package)].into(),
        resolved,
        None,
        frozen,
    )?;
    let cfg = BuildConfig {
        output_style: OutputStyle::Fancy,
        verbose,
        suppress_progress: !verbose,
        ..Default::default()
    };
    let result = session.execute(&cfg, input, &[&meta], MetadataOutput::Compiler)?;
    let code = result.return_code_for_success();
    anyhow::ensure!(
        code == 0,
        "failed to build run target; build exited with code {code}"
    );
    let executable = meta
        .artifacts
        .iter()
        .find(|(artifact, _)| matches!(artifact, ArtifactKey::Executable { .. }))
        .expect("run planning should request an executable artifact")
        .1
        .first()
        .expect("the executable artifact should resolve to one path")
        .clone();
    Ok(StandaloneWasm {
        executable,
        policy_source,
    })
}
