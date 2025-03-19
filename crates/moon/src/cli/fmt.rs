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

use moonbuild::dry_run;
use mooncake::pkg::sync::auto_sync;
use moonutil::{
    common::{BlockStyle, FileLock, FmtOpt, MoonbuildOpt, MooncOpt, RunMode},
    dirs::{mk_arch_mode_dir, PackageDirs},
    mooncakes::{sync::AutoSyncFlags, RegistryConfig},
};

use super::{pre_build::scan_with_pre_build, UniversalFlags};

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
        output_json: false,
        no_parallelize: false,
        parallelism: None,
        use_tcc_run: false,
        all_stubs: vec![],
    };

    let module = scan_with_pre_build(
        false,
        &moonc_opt,
        &moonbuild_opt,
        &resolved_env,
        &dir_sync_result,
    )?;

    if cli.dry_run {
        return dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt);
    }
    moonbuild::entry::run_fmt(&module, &moonc_opt, &moonbuild_opt)
}
