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

use anyhow::Context;
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

use crate::rr_build::{self, BuildConfig, CalcUserIntentOutput};

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
    let resolve_output = sync_and_resolve_bundle_project(&cli, &cmd, &dirs, output.user_log())?;
    let _lock;
    if !cli.dry_run {
        _lock = lock_directory(&dirs.target_dir, output.user_log())?;
    }
    run_bundle_rr_from_resolved(&cli, &cmd, &dirs, &targets, resolve_output, output).with_context(
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
    let resolve_output = sync_and_resolve_bundle_project(cli, cmd, dirs, output.user_log())?;
    let _lock;
    if !cli.dry_run {
        _lock = lock_directory(&dirs.target_dir, output.user_log())?;
    }
    run_bundle_rr_from_resolved(
        cli,
        cmd,
        dirs,
        selected_target_backend.as_slice(),
        resolve_output,
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
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
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
            resolve_output,
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
                    resolve_output.clone(),
                    user_log,
                )
            })
            .collect::<anyhow::Result<Vec<_>>>()?
    };

    if cli.dry_run {
        output.write_result(|writer| {
            let (build_metas, build_inputs): (Vec<_>, Vec<_>) = planned_runs.into_iter().unzip();
            let build_input =
                rr_build::compose_build_inputs(build_inputs).map_err(std::io::Error::other)?;
            rr_build::write_dry_run(
                writer,
                &build_input,
                build_metas.iter().flat_map(|meta| meta.artifacts.values()),
                source_dir,
                target_dir,
            )
        })?;
        Ok(0)
    } else {
        // Generate all_pkgs.json for indirect dependency resolution
        for (build_meta, build_input) in &planned_runs {
            rr_build::generate_all_pkgs_json(build_meta)?;
            // Generate legacy metadata for tooling compatibility
            rr_build::generate_metadata(source_dir, target_dir, build_meta, build_input, None)?;
        }
        let build_inputs = planned_runs.into_iter().map(|(_, input)| input).collect();
        let build_input = rr_build::compose_build_inputs(build_inputs)?;

        let result = rr_build::execute_build(
            &BuildConfig::from_flags(&cmd.build_flags, &cli.unstable_feature, cli.verbose),
            build_input,
            target_dir,
            user_log,
        )?;
        result.print_info(cli.quiet, "bundling")?;
        Ok(result.return_code_for_success())
    }
}

fn sync_and_resolve_bundle_project(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    dirs: &PackageDirs,
    user_log: &UserLog,
) -> anyhow::Result<moonbuild_rupes_recta::ResolveOutput> {
    let resolve_config = moonbuild_rupes_recta::ResolveConfig::new_with_load_defaults(
        cmd.auto_sync_flags.frozen,
        !cmd.build_flags.std(),
        cmd.build_flags.enable_coverage,
        cli.workspace_env.clone(),
    );
    rr_build::sync_and_resolve_project(&resolve_config, dirs, user_log)
}

pub(crate) fn plan_bundle_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    user_log: &UserLog,
) -> anyhow::Result<(rr_build::BuildMeta, rr_build::BuildInput)> {
    let preconfig = bundle_preconfig(cli, cmd, target_dir, selected_target_backend);
    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        user_log,
        &resolve_output,
    )?;
    let intent = bundle_user_intent(&resolve_output);
    rr_build::plan_resolved_build_from_intent(
        preconfig,
        &cli.unstable_feature,
        user_log,
        planning_context,
        intent,
        mooncake_bin_dir,
        resolve_output,
    )
}

fn bundle_preconfig(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    target_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
) -> rr_build::CompilePreConfig {
    let mut preconfig = rr_build::preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        target_dir,
        RunMode::Bundle,
    );
    preconfig.warning_condition = if cmd.build_flags.deny_warn {
        WarningCondition::Deny
    } else {
        WarningCondition::Allow
    };
    preconfig
}

fn bundle_user_intent(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
) -> CalcUserIntentOutput {
    resolve_output
        .local_modules()
        .iter()
        .map(|&module| UserIntent::Bundle(module))
        .collect::<Vec<_>>()
        .into()
}
