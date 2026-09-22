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

use anyhow::Context;
use moonbuild::BuildMeta;
use moonbuild::execution::BuildInput;
use moonbuild_rupes_recta::{build_lower::WarningCondition, intent::UserIntent};
use moonutil::{
    build_options::RunMode,
    cli_support::AutoSyncFlags,
    cli_support::UniversalFlags,
    command_output::CommandOutput,
    locks::lock_directory,
    project::PackageDirs,
    target::{SurfaceTarget, TargetBackend, lower_surface_targets},
    user_log::UserLog,
};
use std::path::Path;
use tracing::instrument;

use crate::rr_build::{self, CalcUserIntentOutput};

use super::BuildFlags;

/// Bundle the module
#[derive(Debug, clap::Parser)]
#[clap(hide(true))]
pub(crate) struct BundleSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    /// Bundle all targets
    #[clap(long)]
    pub all: bool,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,
}

#[instrument(skip_all)]
pub(crate) fn run_bundle(
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
    let resolved_project = prepare_bundle_project(&cli, &cmd, &dirs, output.user_log())?;
    let _lock;
    if !cli.dry_run {
        _lock = lock_directory(&dirs.target_dir, output.user_log())?;
    }
    run_bundle_rr_from_resolved(&cli, &cmd, &dirs, &targets, resolved_project, output).with_context(
        || match targets.as_slice() {
            [target] => format!("failed to run bundle for target {target:?}"),
            _ => format!("failed to run bundle for targets {targets:?}"),
        },
    )
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_bundle_internal(
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
pub(crate) fn run_bundle_internal_rr(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    dirs: &PackageDirs,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let resolved_project = prepare_bundle_project(cli, cmd, dirs, output.user_log())?;
    let _lock;
    if !cli.dry_run {
        _lock = lock_directory(&dirs.target_dir, output.user_log())?;
    }
    run_bundle_rr_from_resolved(
        cli,
        cmd,
        dirs,
        selected_target_backend.as_slice(),
        resolved_project,
        output,
    )
}

/// Plans and executes a bundle from resolved project data.
///
/// The caller must hold the target-directory lock for a non-dry-run build.
fn run_bundle_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    dirs: &PackageDirs,
    selected_target_backends: &[TargetBackend],
    resolved_project: moonbuild_rupes_recta::ResolvedProject,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let user_log = output.user_log();
    let PackageDirs {
        source_dir,
        target_dir,
        ..
    } = dirs;
    let planned_runs = if selected_target_backends.is_empty() {
        vec![plan_bundle_rr_from_resolved(
            cli,
            cmd,
            target_dir,
            &dirs.mooncake_bin_dir,
            None,
            resolved_project,
            user_log,
        )?]
    } else {
        selected_target_backends
            .iter()
            .copied()
            .map(|target| {
                plan_bundle_rr_from_resolved(
                    cli,
                    cmd,
                    target_dir,
                    &dirs.mooncake_bin_dir,
                    Some(target),
                    resolved_project.clone(),
                    user_log,
                )
            })
            .collect::<anyhow::Result<Vec<_>>>()?
    };

    if cli.dry_run {
        output.write_result(|writer| {
            let build_inputs = planned_runs.into_iter().map(|(_, input)| input).collect();
            let build_input = BuildInput::compose(build_inputs).map_err(std::io::Error::other)?;
            rr_build::write_dry_run(writer, &build_input, source_dir)
        })?;
        Ok(0)
    } else {
        // Generate all_pkgs.json for indirect dependency resolution
        for (build_meta, _) in &planned_runs {
            rr_build::generate_all_pkgs_json(build_meta)?;
        }
        let build_inputs = planned_runs.into_iter().map(|(_, input)| input).collect();
        let build_input = BuildInput::compose(build_inputs)?;

        let result = moonbuild::execution::execute_build(
            &cmd.build_flags
                .execution_config(&cli.unstable_feature, cli.verbose),
            build_input,
            target_dir,
            user_log,
        )?;
        result.print_info(cli.quiet, "bundling")?;
        Ok(result.return_code_for_success())
    }
}

fn prepare_bundle_project(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    dirs: &PackageDirs,
    user_log: &UserLog,
) -> anyhow::Result<moonbuild_rupes_recta::ResolvedProject> {
    let preparation_config =
        moonbuild_rupes_recta::ProjectPreparationConfig::new_with_load_defaults(
            cmd.auto_sync_flags.frozen,
            !cmd.build_flags.std(),
            cmd.build_flags.enable_coverage,
            cli.workspace_env.clone(),
        );
    let discovered = rr_build::sync_and_discover_project(&preparation_config, dirs, user_log)?;
    Ok(discovered.resolve_packages(user_log)?)
}

pub(crate) fn plan_bundle_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    resolved_project: moonbuild_rupes_recta::ResolvedProject,
    user_log: &UserLog,
) -> anyhow::Result<(BuildMeta, BuildInput)> {
    let mut compile_config = rr_build::prepare_resolved_build(
        cli,
        &cmd.build_flags,
        selected_target_backend,
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
    rr_build::plan_resolved_build_from_intent(
        compile_config,
        user_log,
        intent,
        mooncake_bin_dir,
        resolved_project,
        cmd.build_flags.jobs,
        cmd.auto_sync_flags.frozen,
        cli.dry_run,
    )
}

fn bundle_user_intent(
    discovered: &moonbuild_rupes_recta::DiscoveredProject,
) -> CalcUserIntentOutput {
    discovered
        .local_modules()
        .iter()
        .map(|&module| UserIntent::Bundle(module))
        .collect::<Vec<_>>()
        .into()
}
