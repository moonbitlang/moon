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
use mooncake::pkg::sync::auto_sync;
use moonutil::{
    cli::UniversalFlags,
    common::{
        lower_surface_targets, FileLock, MoonbuildOpt, RunMode, SurfaceTarget, TargetBackend,
    },
    dirs::{mk_arch_mode_dir, PackageDirs},
    mooncakes::{sync::AutoSyncFlags, RegistryConfig},
};
use std::{path::Path, sync::Arc, thread};

use super::{pre_build::scan_with_pre_build, BuildFlags};

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
    // this is a workaround for supporting bundle core for native backend when --target all
    // should move to `lower_surface_targets` when native backend being stable
    if cmd.all || cmd.build_flags.target == Some(vec![SurfaceTarget::All]) {
        targets.push(TargetBackend::Native);
    }

    let mut ret_value = 0;
    if cmd.build_flags.serial {
        for t in targets {
            let mut cmd = cmd.clone();
            cmd.build_flags.target_backend = Some(t);
            let x = run_bundle_internal(&cli, &cmd, &source_dir, &target_dir)
                .context(format!("failed to run bundle for target {:?}", t))?;
            ret_value = ret_value.max(x);
        }
    } else {
        let cli = Arc::new(cli);
        let source_dir = Arc::new(source_dir);
        let target_dir = Arc::new(target_dir);
        let mut handles = Vec::new();

        for t in targets {
            let cli = Arc::clone(&cli);
            let mut cmd = cmd.clone();
            cmd.build_flags.target_backend = Some(t);
            let source_dir = Arc::clone(&source_dir);
            let target_dir = Arc::clone(&target_dir);

            let handle =
                thread::spawn(move || run_bundle_internal(&cli, &cmd, &source_dir, &target_dir));

            handles.push((t, handle));
        }

        for (backend, handle) in handles {
            let x = handle
                .join()
                .unwrap()
                .context(format!("failed to run bundle for target {:?}", backend))?;
            ret_value = ret_value.max(x);
        }
    }
    Ok(ret_value)
}

fn run_bundle_internal(
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
    )?;

    let run_mode = RunMode::Bundle;
    let moonc_opt = super::get_compiler_flags(source_dir, &cmd.build_flags)?;
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
        output_json: false,
        no_parallelize: false,
        build_graph: false,
        parallelism: cmd.build_flags.jobs,
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
    moonbuild::entry::run_bundle(&module, &moonbuild_opt, &moonc_opt)
}
