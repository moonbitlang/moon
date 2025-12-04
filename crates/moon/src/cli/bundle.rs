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
use moonbuild::dry_run;
use moonbuild_rupes_recta::{build_lower::WarningCondition, intent::UserIntent};
use mooncake::pkg::sync::auto_sync;
use moonutil::{
    cli::UniversalFlags,
    common::{
        FileLock, MoonbuildOpt, PrePostBuild, RunMode, SurfaceTarget, TargetBackend,
        lower_surface_targets,
    },
    cond_expr::OptLevel,
    dirs::{PackageDirs, mk_arch_mode_dir},
    mooncakes::{RegistryConfig, sync::AutoSyncFlags},
};
use std::path::Path;
use tracing::{Level, instrument};

use crate::rr_build::{self, BuildConfig};

use super::{BuildFlags, pre_build::scan_with_x_build};

/// Bundle the module
#[derive(Debug, clap::Parser, Clone)]
#[clap(hide(true))]
pub struct BundleSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    /// Bundle all targets
    #[clap(long)]
    pub all: bool,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,
}

#[instrument(skip_all)]
pub fn run_bundle(cli: UniversalFlags, cmd: BundleSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let target = if cmd.all {
        Some(vec![SurfaceTarget::All])
    } else {
        cmd.build_flags.target.clone()
    };

    if target.is_none() {
        return run_bundle_internal(&cli, &cmd, &source_dir, &target_dir);
    }

    let mut surface_targets = target.clone().unwrap();
    if cmd.all {
        surface_targets.push(SurfaceTarget::All);
    }
    let mut targets = lower_surface_targets(&surface_targets);
    // this is a workaround for supporting bundle core for native & llvm backend when --target all
    // should move to `lower_surface_targets` when native backend being stable
    if cmd.all || cmd.build_flags.target == Some(vec![SurfaceTarget::All]) {
        targets.push(TargetBackend::Native);
    }

    let mut ret_value = 0;
    for t in targets {
        let mut cmd = cmd.clone();
        cmd.build_flags.target_backend = Some(t);
        let x = run_bundle_internal(&cli, &cmd, &source_dir, &target_dir)
            .context(format!("failed to run bundle for target {t:?}"))?;
        ret_value = ret_value.max(x);
    }
    Ok(ret_value)
}

#[instrument(skip_all)]
pub fn run_bundle_internal(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    source_dir: &Path,
    target_dir: &Path,
) -> anyhow::Result<i32> {
    if cli.unstable_feature.rupes_recta {
        run_bundle_internal_rr(cli, cmd, source_dir, target_dir)
    } else {
        run_bundle_internal_legacy(cli, cmd, source_dir, target_dir)
    }
}

#[instrument(skip_all)]
pub fn run_bundle_internal_rr(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    source_dir: &Path,
    target_dir: &Path,
) -> anyhow::Result<i32> {
    let mut preconfig = rr_build::preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        target_dir,
        OptLevel::Release,
        RunMode::Bundle,
    );
    // Allow warn in `moon bundle`, different from other run modes, to reduce
    // commandline clutter on installation
    preconfig.warning_condition = if cmd.build_flags.deny_warn {
        WarningCondition::Deny
    } else {
        WarningCondition::Allow
    };

    let (_build_meta, build_graph) = rr_build::plan_build(
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
            _build_meta.artifacts.values(),
            source_dir,
            target_dir,
        );
        Ok(0)
    } else {
        let _lock = FileLock::lock(target_dir)?;

        // Generate metadata for IDE & bundler
        rr_build::generate_metadata(source_dir, target_dir, &_build_meta, RunMode::Bundle, None)?;

        let result = rr_build::execute_build(
            &BuildConfig::from_flags(&cmd.build_flags, &cli.unstable_feature, cli.verbose),
            build_graph,
            target_dir,
        )?;
        result.print_info(cli.quiet, "bundling")?;
        Ok(result.return_code_for_success())
    }
}

#[instrument(level = Level::DEBUG, skip_all)]
fn run_bundle_internal_legacy(
    cli: &UniversalFlags,
    cmd: &BundleSubcommand,
    source_dir: &Path,
    target_dir: &Path,
) -> anyhow::Result<i32> {
    // Run moon install before build
    let (resolved_env, dir_sync_result) = auto_sync(
        source_dir,
        &cmd.auto_sync_flags,
        &RegistryConfig::load(),
        cli.quiet,
        true, // Legacy don't need std injection
    )?;

    let run_mode = RunMode::Bundle;
    let mut moonc_opt = super::get_compiler_flags(source_dir, &cmd.build_flags)?;
    // Legacy path: allow all warnings for `moon bundle` unless explicitly denied via --deny-warn
    moonc_opt.build_opt.deny_warn = cmd.build_flags.deny_warn;
    let sort_input = cmd.build_flags.sort_input;

    let raw_target_dir = target_dir.to_path_buf();
    let target_dir = mk_arch_mode_dir(source_dir, target_dir, &moonc_opt, run_mode)?;
    let _lock = FileLock::lock(&target_dir)?;

    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.to_path_buf(),
        raw_target_dir,
        target_dir,
        sort_input,
        run_mode,
        test_opt: None,
        check_opt: None,
        build_opt: None,
        fmt_opt: None,
        args: vec![],
        verbose: cli.verbose,
        quiet: cli.quiet,
        no_render_output: cmd.build_flags.output_style().needs_no_render(),
        no_parallelize: false,
        build_graph: false,
        parallelism: cmd.build_flags.jobs,
        use_tcc_run: false,
        dynamic_stub_libs: None,
        render_no_loc: cmd.build_flags.render_no_loc,
    };
    let module = scan_with_x_build(
        false,
        &moonc_opt,
        &moonbuild_opt,
        &resolved_env,
        &dir_sync_result,
        &PrePostBuild::PreBuild,
    )?;

    if cli.dry_run {
        return dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt);
    }

    moonbuild::entry::run_bundle(&module, &moonbuild_opt, &moonc_opt)
}
