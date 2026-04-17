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
        project_manifest_path,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

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
            project_manifest_path.as_deref(),
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
            project_manifest_path.as_deref(),
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
    project_manifest_path: Option<&Path>,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    run_bundle_internal_rr(
        cli,
        cmd,
        source_dir,
        target_dir,
        mooncakes_dir,
        project_manifest_path,
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
    project_manifest_path: Option<&Path>,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new_with_load_defaults(
        cmd.auto_sync_flags.frozen,
        !cmd.build_flags.std(),
        cmd.build_flags.enable_coverage,
    )
    .with_project_manifest_path(project_manifest_path);
    let resolve_output = moonbuild_rupes_recta::resolve(&resolve_cfg, source_dir, mooncakes_dir)?;
    let groups = if let Some(target_backend) = selected_target_backend {
        vec![(target_backend, resolve_output.local_modules().to_vec())]
    } else {
        rr_build::group_modules_by_default_target(&resolve_output, resolve_output.local_modules())
    };

    let mut planned = Vec::new();
    for (target_backend, module_scope) in groups {
        let mut preconfig = rr_build::preconfig_compile(
            &cmd.auto_sync_flags,
            cli,
            &cmd.build_flags,
            Some(target_backend),
            target_dir,
            RunMode::Bundle,
        );
        preconfig.warning_condition = if cmd.build_flags.deny_warn {
            WarningCondition::Deny
        } else {
            WarningCondition::Allow
        };

        let (build_meta, build_graph) = rr_build::plan_build_from_resolved(
            preconfig,
            &cli.unstable_feature,
            target_dir,
            UserDiagnostics::from_flags(cli),
            Box::new(move |_, _tb| {
                Ok(module_scope
                    .iter()
                    .map(|&mid| UserIntent::Bundle(mid))
                    .collect::<Vec<_>>()
                    .into())
            }),
            resolve_output.clone(),
        )?;
        planned.push((build_meta, build_graph));
    }

    if cli.dry_run {
        for (build_meta, build_graph) in &planned {
            rr_build::print_dry_run(
                build_graph,
                build_meta.artifacts.values(),
                source_dir,
                target_dir,
            );
        }
        Ok(0)
    } else {
        let _lock = FileLock::lock(target_dir)?;
        let build_config = BuildConfig::from_flags(
            &cmd.build_flags,
            &cli.unstable_feature,
            cli.verbose,
            UserDiagnostics::from_flags(cli),
        );
        let mut ret = 0;
        for (build_meta, build_graph) in planned {
            rr_build::generate_all_pkgs_json(target_dir, &build_meta, RunMode::Bundle)?;
            rr_build::generate_metadata(
                source_dir,
                target_dir,
                &build_meta,
                RunMode::Bundle,
                None,
            )?;
            let result = rr_build::execute_build(&build_config, build_graph, target_dir)?;
            result.print_info(cli.quiet, "bundling")?;
            ret = ret.max(result.return_code_for_success());
        }
        Ok(ret)
    }
}
