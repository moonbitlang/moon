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
use moonbuild::watch::watching;
use moonbuild::watcher_is_running;
use moonbuild::{entry, MOON_PID_NAME};
use mooncake::pkg::sync::auto_sync;
use moonutil::cli::UniversalFlags;
use moonutil::common::FileLock;
use moonutil::common::MoonbuildOpt;
use moonutil::common::RunMode;
use moonutil::common::WATCH_MODE_DIR;
use moonutil::common::{lower_surface_targets, CheckOpt};
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::dirs::PackageDirs;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use n2::trace;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use super::pre_build::scan_with_pre_build;
use super::{get_compiler_flags, BuildFlags};

/// Check the current package, but don't build object files
#[derive(Debug, clap::Parser, Clone)]
pub struct CheckSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    /// Output in json format
    #[clap(long)]
    pub output_json: bool,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Monitor the file system and automatically check files
    #[clap(long, short)]
    pub watch: bool,

    /// The package(and it's deps) to check
    pub package_path: Option<PathBuf>,

    /// The patch file to check, Only valid when checking specified package.
    #[clap(long, requires = "package_path")]
    pub patch_file: Option<PathBuf>,

    /// Whether to skip the mi generation, Only valid when checking specified package.
    #[clap(long, requires = "package_path")]
    pub no_mi: bool,

    /// Whether to explain the error code with details.
    #[clap(long)]
    pub explain: bool,
}

pub fn run_check(cli: &UniversalFlags, cmd: &CheckSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        mut target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    // make a dedicated directory for the watch mode so that we don't block(MOON_LOCK) the normal no-watch mode(automatically trigger by ide in background)
    if cmd.watch {
        target_dir = target_dir.join(WATCH_MODE_DIR);
        std::fs::create_dir_all(&target_dir).context(format!(
            "Failed to create target directory: '{}'",
            target_dir.display()
        ))?;
    };

    if cmd.build_flags.target.is_none() {
        return run_check_internal(cli, cmd, &source_dir, &target_dir);
    }

    let surface_targets = cmd.build_flags.target.clone().unwrap();
    let targets = lower_surface_targets(&surface_targets);

    let mut ret_value = 0;
    if cmd.build_flags.serial {
        for t in targets {
            let mut cmd = (*cmd).clone();
            cmd.build_flags.target_backend = Some(t);
            let x = run_check_internal(cli, &cmd, &source_dir, &target_dir)
                .context(format!("failed to run check for target {:?}", t))?;
            ret_value = ret_value.max(x);
        }
    } else {
        let cli = Arc::new(cli.clone());
        let source_dir = Arc::new(source_dir);
        let target_dir = Arc::new(target_dir);
        let mut handles = Vec::new();

        for t in &targets {
            let cli = Arc::clone(&cli);
            let mut cmd = (*cmd).clone();
            cmd.build_flags.target_backend = Some(*t);
            let source_dir = Arc::clone(&source_dir);
            let target_dir = Arc::clone(&target_dir);

            let handle =
                thread::spawn(move || run_check_internal(&cli, &cmd, &source_dir, &target_dir));

            handles.push((*t, handle));
        }

        for (backend, handle) in handles {
            let x = handle
                .join()
                .unwrap()
                .context(format!("failed to run check for target {:?}", backend))?;
            ret_value = ret_value.max(x);
        }
    }
    Ok(ret_value)
}

fn run_check_internal(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
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

    let raw_target_dir = target_dir;
    let run_mode = RunMode::Check;
    let mut moonc_opt = get_compiler_flags(source_dir, &cmd.build_flags)?;
    moonc_opt.build_opt.deny_warn = cmd.build_flags.deny_warn;
    let target_dir = mk_arch_mode_dir(source_dir, target_dir, &moonc_opt, run_mode)?;
    let _lock = FileLock::lock(&target_dir)?;

    let sort_input = cmd.build_flags.sort_input;

    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.to_path_buf(),
        raw_target_dir: raw_target_dir.to_path_buf(),
        target_dir: target_dir.clone(),
        sort_input,
        run_mode,
        quiet: cli.quiet,
        verbose: cli.verbose,
        output_json: cmd.output_json,
        build_graph: cli.build_graph,
        check_opt: Some(CheckOpt {
            package_path: cmd.package_path.clone(),
            patch_file: cmd.patch_file.clone(),
            no_mi: cmd.no_mi,
            explain: cmd.explain,
        }),
        test_opt: None,
        build_opt: None,
        fmt_opt: None,
        args: vec![],
        no_parallelize: false,
        parallelism: cmd.build_flags.jobs,
    };

    let mut module = scan_with_pre_build(
        false,
        &moonc_opt,
        &moonbuild_opt,
        &resolved_env,
        &dir_sync_result,
    )?;

    if let Some(CheckOpt {
        package_path: Some(pkg_path),
        patch_file: pp,
        no_mi: nm,
        ..
    }) = moonbuild_opt.check_opt.as_ref()
    {
        let pkg_by_path = module.get_package_by_path_mut(&moonbuild_opt.source_dir.join(pkg_path));
        if let Some(specified_pkg) = pkg_by_path {
            specified_pkg.no_mi = *nm;
            specified_pkg.patch_file = pp.clone();
        }
    };

    if cli.dry_run {
        return dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt);
    }

    if cli.trace {
        trace::open("trace.json").context("failed to open `trace.json`")?;
    }

    let watch_mode = cmd.watch;

    let res = if watch_mode {
        let reg_cfg = RegistryConfig::load();
        watching(
            &moonc_opt,
            &moonbuild_opt,
            &reg_cfg,
            &module,
            raw_target_dir,
        )
    } else {
        let pid_path = target_dir.join(MOON_PID_NAME);
        let running = watcher_is_running(&pid_path);

        if let Ok(true) = running {
            let output_path = target_dir.join("check.output");
            let output = std::fs::read_to_string(&output_path)
                .context(format!("failed to open `{}`", output_path.display()))?;
            if !output.trim().is_empty() {
                println!("{}", output.trim());
            }
            Ok(if output.is_empty() { 0 } else { 1 })
        } else {
            entry::run_check(&moonc_opt, &moonbuild_opt, &module)
        }
    };

    if cli.trace {
        trace::close();
    }

    res
}
