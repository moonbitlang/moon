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
    cli::UniversalFlags,
    common::{FileLock, RunMode, SurfaceTarget, TargetBackend, lower_surface_targets},
    dirs::PackageDirs,
    mooncakes::sync::AutoSyncFlags,
};
use std::path::Path;
use tracing::instrument;

use crate::rr_build::{self, BuildConfig};

use super::BuildFlags;

/// Bundle the module
#[derive(Debug, clap::Parser, Clone)]
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
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let mut surface_targets = cmd.build_flags.target.clone();
    if cmd.all {
        surface_targets.push(SurfaceTarget::All);
    }

    if surface_targets.is_empty() {
        return run_bundle_internal(&cli, &cmd, &source_dir, &target_dir, None);
    }

    let mut targets = lower_surface_targets(&surface_targets);
    // this is a workaround for supporting bundle core for native & llvm backend when --target all
    // should move to `lower_surface_targets` when native backend being stable
    if cmd.all || cmd.build_flags.target == vec![SurfaceTarget::All] {
        targets.push(TargetBackend::Native);
    }

    let mut ret_value = 0;
    for t in targets {
        let x = run_bundle_internal(&cli, &cmd, &source_dir, &target_dir, Some(t))
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
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    run_bundle_internal_rr(cli, cmd, source_dir, target_dir, selected_target_backend)
}

#[instrument(skip_all)]
pub(crate) fn run_bundle_internal_rr(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    let mut preconfig = rr_build::preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        target_dir,
        RunMode::Bundle,
    );

    // Allow warn in `moon bundle`, different from other run modes, to reduce
    // commandline clutter on installation
    preconfig.warning_condition = if cmd.build_flags.deny_warn {
        WarningCondition::Deny
    } else {
        WarningCondition::Allow
    };

    let (build_meta, build_graph) = rr_build::plan_build(
        preconfig,
        &cli.unstable_feature,
        source_dir,
        target_dir,
        Box::new(|r, _tb| {
            Ok(r.local_modules()
                .iter()
                .map(|&mid| UserIntent::Bundle(mid))
                .collect::<Vec<_>>()
                .into())
        }),
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
        rr_build::generate_all_pkgs_json(target_dir, &build_meta, RunMode::Bundle)?;
        // Generate metadata for IDE & bundler
        rr_build::generate_metadata(source_dir, target_dir, &build_meta, RunMode::Bundle, None)?;

        let result = rr_build::execute_build(
            &BuildConfig::from_flags(&cmd.build_flags, &cli.unstable_feature, cli.verbose),
            build_graph,
            target_dir,
        )?;
        result.print_info(cli.quiet, "bundling")?;
        Ok(result.return_code_for_success())
    }
}
