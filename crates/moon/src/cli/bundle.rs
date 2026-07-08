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
    locks::FileLock,
    project::{PackageDirs, ProjectManifest},
    target::{SurfaceTarget, TargetBackend, lower_surface_targets},
};
use std::path::Path;
use tracing::instrument;

use crate::rr_build::{self, BuildConfig, CalcUserIntentOutput};
use crate::user_diagnostics::UserDiagnostics;

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
pub(crate) fn run_bundle(cli: UniversalFlags, cmd: BundleSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
        mooncakes_dir,
        project_manifest,
    } = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .package_dirs()?;

    let mut surface_targets = cmd.build_flags.target.clone();
    if cmd.all {
        surface_targets.push(SurfaceTarget::All);
    }

    if surface_targets.is_empty() {
        return run_bundle_internal(
            &cli,
            &cmd,
            &source_dir,
            &target_dir,
            &mooncakes_dir,
            &project_manifest,
            None,
        );
    }

    let targets = lower_surface_targets(&surface_targets);

    let mut ret_value = 0;
    for t in targets {
        let x = run_bundle_internal(
            &cli,
            &cmd,
            &source_dir,
            &target_dir,
            &mooncakes_dir,
            &project_manifest,
            Some(t),
        )
        .context(format!("failed to run bundle for target {t:?}"))?;
        ret_value = ret_value.max(x);
    }
    Ok(ret_value)
}

#[instrument(skip_all)]
pub(crate) fn run_bundle_internal(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    mooncakes_dir: &Path,
    project_manifest: &ProjectManifest,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    run_bundle_internal_rr(
        cli,
        cmd,
        source_dir,
        target_dir,
        mooncakes_dir,
        project_manifest,
        selected_target_backend,
    )
}

#[instrument(skip_all)]
pub(crate) fn run_bundle_internal_rr(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    mooncakes_dir: &Path,
    project_manifest: &ProjectManifest,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    let (build_meta, build_graph) = plan_bundle_rr(
        cli,
        cmd,
        source_dir,
        target_dir,
        mooncakes_dir,
        project_manifest,
        selected_target_backend,
    )?;

    if cli.dry_run {
        rr_build::print_dry_run(
            &build_graph,
            build_meta.artifacts.values(),
            source_dir,
            target_dir,
        );
        Ok(0)
    } else {
        let _lock = FileLock::lock(target_dir)?;
        // Generate all_pkgs.json for indirect dependency resolution
        rr_build::generate_all_pkgs_json(&build_meta)?;
        // Generate metadata for IDE & bundler
        rr_build::generate_metadata(source_dir, target_dir, &build_meta, &build_graph, None)?;

        let result = rr_build::execute_build(
            &BuildConfig::from_flags(
                &cmd.build_flags,
                &cli.unstable_feature,
                cli.verbose,
                UserDiagnostics::from_flags(cli),
            ),
            build_graph,
            target_dir,
        )?;
        result.print_info(cli.quiet, "bundling")?;
        Ok(result.return_code_for_success())
    }
}

pub(crate) fn plan_bundle_rr(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    mooncakes_dir: &Path,
    project_manifest: &ProjectManifest,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<(rr_build::BuildMeta, rr_build::BuildInput)> {
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new_with_load_defaults(
        cmd.auto_sync_flags.frozen,
        !cmd.build_flags.std(),
        cmd.build_flags.enable_coverage,
        cli.workspace_env.clone(),
    );
    let mooncake_bin_dir = mooncakes_dir.join(moonutil::constants::MOON_BIN_DIR);
    let synced_env = moonbuild_rupes_recta::sync_dependencies(
        &resolve_cfg,
        source_dir,
        mooncakes_dir,
        project_manifest,
    )?;
    let resolve_output = moonbuild_rupes_recta::resolve_synced_project(&resolve_cfg, synced_env)?;
    plan_bundle_rr_from_resolved(
        cli,
        cmd,
        target_dir,
        &mooncake_bin_dir,
        selected_target_backend,
        resolve_output,
    )
}

pub(crate) fn plan_bundle_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
) -> anyhow::Result<(rr_build::BuildMeta, rr_build::BuildInput)> {
    let preconfig = bundle_preconfig(cli, cmd, target_dir, selected_target_backend);
    let output = UserDiagnostics::from_flags(cli);
    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        output,
        &resolve_output,
    )?;
    let intent = bundle_user_intent(&resolve_output);
    rr_build::plan_resolved_build_from_intent(
        preconfig,
        &cli.unstable_feature,
        output,
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
