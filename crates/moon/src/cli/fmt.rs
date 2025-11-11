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
use moonbuild_rupes_recta::fmt::FmtConfig;
use mooncake::pkg::sync::auto_sync;
use moonutil::{
    common::{
        BlockStyle, DiagnosticLevel, FileLock, FmtOpt, MoonbuildOpt, MooncOpt, PrePostBuild,
        RunMode,
    },
    dirs::{PackageDirs, mk_arch_mode_dir},
    mooncakes::{RegistryConfig, sync::AutoSyncFlags},
};

use crate::rr_build::{self, BuildConfig, plan_fmt};

use super::{UniversalFlags, pre_build::scan_with_x_build};

/// Format source code
#[derive(Debug, clap::Parser)]
pub struct FmtSubcommand {
    /// Check only and don't change the source code
    #[clap(long)]
    check: bool,

    /// Sort input files
    #[clap(long)]
    pub sort_input: bool,

    /// Add separator between each segments
    #[clap(long, value_enum, num_args=0..=1, default_missing_value = "true")]
    pub block_style: Option<BlockStyle>,
    pub args: Vec<String>,
}

pub fn run_fmt(cli: &UniversalFlags, cmd: FmtSubcommand) -> anyhow::Result<i32> {
    if cli.unstable_feature.rupes_recta {
        run_fmt_rr(cli, cmd)
    } else {
        run_fmt_legacy(cli, cmd)
    }
}

fn run_fmt_rr(cli: &UniversalFlags, cmd: FmtSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let resolved = moonbuild_rupes_recta::fmt::resolve_for_fmt(&source_dir)
        .context("Failed to resolve environment")?;
    let fmt_config = FmtConfig {
        block_style: cmd.block_style.unwrap_or_default().is_line(),
        check_only: cmd.check,
        extra_args: cmd.args.clone(),
    };
    let graph = plan_fmt(&resolved, &fmt_config, &target_dir)?;

    if cli.dry_run {
        rr_build::print_dry_run_all(&graph, &source_dir, &target_dir);
        Ok(0)
    } else {
        let res = rr_build::execute_build(&BuildConfig::default(), graph, &target_dir)?;
        res.print_info(cli.quiet, "formatting")?;
        Ok(res.return_code_for_success())
    }
}

fn run_fmt_legacy(cli: &UniversalFlags, cmd: FmtSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let moonc_opt = MooncOpt::default();
    let run_mode = RunMode::Format;
    let raw_target_dir = target_dir.to_path_buf();
    let target_dir = mk_arch_mode_dir(&source_dir, &target_dir, &moonc_opt, run_mode)?;
    let _lock = FileLock::lock(&target_dir)?;

    // Resolve dependencies, but don't download anything
    let (resolved_env, dir_sync_result) = auto_sync(
        &source_dir,
        &AutoSyncFlags { frozen: true },
        &RegistryConfig::load(),
        cli.quiet,
        true, // Legacy don't need std injection
    )?;

    let moonbuild_opt = MoonbuildOpt {
        source_dir,
        raw_target_dir,
        target_dir: target_dir.clone(),
        sort_input: cmd.sort_input,
        run_mode,
        fmt_opt: Some(FmtOpt {
            check: cmd.check,
            block_style: cmd.block_style.unwrap_or_default(),
            extra_args: cmd.args,
        }),
        build_graph: cli.build_graph,
        test_opt: None,
        check_opt: None,
        build_opt: None,
        args: vec![],
        verbose: cli.verbose,
        quiet: cli.quiet,
        no_render_output: false,
        no_parallelize: false,
        parallelism: None,
        use_tcc_run: false,
        dynamic_stub_libs: None,
        render_no_loc: DiagnosticLevel::default(),
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

    moonbuild::entry::run_fmt(&module, &moonc_opt, &moonbuild_opt)
}
