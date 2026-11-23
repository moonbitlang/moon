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

use crate::cli::UniversalFlags;
use crate::cli::target::{SurfaceTarget, lower_surface_targets};
use anyhow::Context;
use moonbuild::BuildMeta;
use moonbuild::compile::{BuildRequest, BuildSession, MetadataOutput};
use moonbuild::execution::BuildInput;
use moonbuild_rupes_recta::{RunMode, build_lower::WarningCondition, intent::UserIntent};
use mooncake::cli::AutoSyncFlags;
use moonutil::{
    command_output::CommandOutput, project::PackageDirs, target::TargetBackend, user_log::UserLog,
};
use std::path::Path;
use tracing::instrument;

use super::BuildFlags;

/// Bundle the module
#[derive(Debug, clap::Parser)]
#[clap(hide(true))]
pub(super) struct BundleSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    /// Bundle all targets
    #[clap(long)]
    pub all: bool,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,
}

#[instrument(skip_all)]
pub(super) fn run_bundle(
    cli: UniversalFlags,
    cmd: BundleSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let dirs = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(output.user_log())?
        .package_dirs()?;

    let mut surface_targets = cmd.build_flags.target.clone();
    if cmd.all {
        surface_targets.push(SurfaceTarget::All);
    }

    if surface_targets.is_empty() {
        return run_bundle_internal(&cli, &cmd, &dirs, None, output);
    }

    let targets = lower_surface_targets(&surface_targets);
    let discovered = sync_and_discover_bundle_project(&cmd, &dirs, output.user_log())?;
    let session = BuildSession::new(&dirs.target_dir, cli.dry_run, output.user_log())?;
    run_bundle_rr_from_discovered(&session, &cli, &cmd, &dirs, &targets, discovered, output)
        .with_context(|| match targets.as_slice() {
            [target] => format!("failed to run bundle for target {target:?}"),
            _ => format!("failed to run bundle for targets {targets:?}"),
        })
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
pub(super) fn run_bundle_internal(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    dirs: &PackageDirs,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    run_bundle_internal_rr(cli, cmd, dirs, selected_target_backend, output)
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
pub(super) fn run_bundle_internal_rr(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    dirs: &PackageDirs,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let discovered = sync_and_discover_bundle_project(cmd, dirs, output.user_log())?;
    let session = BuildSession::new(&dirs.target_dir, cli.dry_run, output.user_log())?;
    run_bundle_rr_from_discovered(
        &session,
        cli,
        cmd,
        dirs,
        selected_target_backend.as_slice(),
        discovered,
        output,
    )
}

/// Plans and executes a bundle from a discovered project.
///
/// The supplied session protects planning and execution with the target-directory lock.
fn run_bundle_rr_from_discovered(
    session: &BuildSession,
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    dirs: &PackageDirs,
    selected_target_backends: &[TargetBackend],
    discovered: moonbuild_rupes_recta::DiscoveredProject,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let user_log = output.user_log();
    let PackageDirs {
        source_dir,
        target_dir,
        ..
    } = dirs;
    let planned_runs = plan_bundle_rr_from_discovered(
        session,
        cli,
        cmd,
        target_dir,
        selected_target_backends,
        discovered,
        user_log,
    )?;

    if cli.dry_run {
        output.write_result(|writer| {
            let build_inputs = planned_runs.into_iter().map(|(_, input)| input).collect();
            let build_input = BuildInput::compose(build_inputs).map_err(std::io::Error::other)?;
            crate::dry_run::write_dry_run(writer, &build_input, source_dir)
        })?;
        Ok(0)
    } else {
        let (build_metas, build_inputs): (Vec<_>, Vec<_>) = planned_runs.into_iter().unzip();
        let build_input = BuildInput::compose(build_inputs)?;

        let result = session.execute(
            &cmd.build_flags
                .execution_config(&cli.unstable_feature, cli.verbose),
            build_input,
            &build_metas.iter().collect::<Vec<_>>(),
            MetadataOutput::Compiler,
        )?;
        result.print_info(cli.quiet, "bundling")?;
        Ok(result.return_code_for_success())
    }
}

fn sync_and_discover_bundle_project(
    cmd: &BundleSubcommand,
    dirs: &PackageDirs,
    user_log: &UserLog,
) -> anyhow::Result<moonbuild_rupes_recta::DiscoveredProject> {
    let preparation_config = moonbuild::prepare::ProjectPreparationConfig::new(
        cmd.auto_sync_flags.frozen,
        cmd.build_flags.enable_coverage,
    )
    .with_jobs(cmd.build_flags.jobs);
    moonbuild::prepare::sync_and_discover_project(&preparation_config, dirs, user_log)
}

/// Selects backends, resolves packages, then plans one bundle per backend.
pub(super) fn plan_bundle_rr_from_discovered(
    session: &BuildSession,
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    target_dir: &Path,
    selected_target_backends: &[TargetBackend],
    discovered: moonbuild_rupes_recta::DiscoveredProject,
    user_log: &UserLog,
) -> anyhow::Result<Vec<(BuildMeta, BuildInput)>> {
    let target_backends = if selected_target_backends.is_empty() {
        vec![
            super::selection::local_modules_preferred_target(&discovered, user_log)
                .unwrap_or_default(),
        ]
    } else {
        selected_target_backends.to_vec()
    };
    let resolved_project = discovered.resolve_packages(&target_backends, user_log)?;
    target_backends
        .into_iter()
        .map(|target_backend| {
            plan_bundle_rr_from_resolved(
                session,
                cli,
                cmd,
                target_dir,
                target_backend,
                resolved_project.clone(),
                user_log,
            )
        })
        .collect()
}

fn plan_bundle_rr_from_resolved(
    session: &BuildSession,
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    target_dir: &Path,
    target_backend: TargetBackend,
    resolved_project: moonbuild_rupes_recta::ResolvedProject,
    user_log: &UserLog,
) -> anyhow::Result<(BuildMeta, BuildInput)> {
    let mut compile_config = cmd.build_flags.compile_config(
        cli,
        target_backend,
        target_dir,
        RunMode::Bundle,
        user_log,
        &resolved_project,
    )?;
    compile_config.warning_condition = if cmd.build_flags.deny_warn {
        WarningCondition::Deny
    } else {
        WarningCondition::Allow
    };
    let intent = bundle_user_intent(&resolved_project);
    session.plan(
        compile_config,
        intent,
        resolved_project,
        cmd.build_flags.jobs,
        cmd.auto_sync_flags.frozen,
    )
}

fn bundle_user_intent(discovered: &moonbuild_rupes_recta::DiscoveredProject) -> BuildRequest {
    discovered
        .local_modules()
        .iter()
        .map(|&module| UserIntent::Bundle(module))
        .collect::<Vec<_>>()
        .into()
}
