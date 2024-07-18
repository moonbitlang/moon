use anyhow::Context;
use moonbuild::dry_run;
use moonbuild::entry;
use mooncake::pkg::sync::auto_sync;
use moonutil::common::lower_surface_targets;
use moonutil::common::FileLock;
use moonutil::common::MoonbuildOpt;
use moonutil::common::RunMode;
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::dirs::PackageDirs;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use n2::trace;
use std::path::Path;
use std::sync::Arc;
use std::thread;

use super::{BuildFlags, UniversalFlags};

/// Build the current package
#[derive(Debug, clap::Parser, Clone)]
pub struct BuildSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,
}

pub fn run_build(cli: &UniversalFlags, cmd: &BuildSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;
    let _lock = FileLock::lock(&target_dir)?;
    if cmd.build_flags.target.is_none() {
        return run_build_internal(cli, cmd, &source_dir, &target_dir);
    }
    let surface_targets = cmd.build_flags.target.clone().unwrap();
    let targets = lower_surface_targets(&surface_targets);

    if cmd.build_flags.serial {
        for t in targets {
            let mut cmd = (*cmd).clone();
            cmd.build_flags.target_backend = Some(t);
            run_build_internal(cli, &cmd, &source_dir, &target_dir)
                .context(format!("failed to run build for target {:?}", t))?;
        }
    } else {
        let cli = Arc::new(cli.clone());
        let source_dir = Arc::new(source_dir);
        let target_dir = Arc::new(target_dir);
        let mut handles = Vec::new();

        for t in targets {
            let cli = Arc::clone(&cli);
            let mut cmd = (*cmd).clone();
            cmd.build_flags.target_backend = Some(t);
            let source_dir = Arc::clone(&source_dir);
            let target_dir = Arc::clone(&target_dir);

            let handle =
                thread::spawn(move || run_build_internal(&cli, &cmd, &source_dir, &target_dir));

            handles.push((t, handle));
        }

        for (backend, handle) in handles {
            handle
                .join()
                .unwrap()
                .context(format!("failed to run build for target {:?}", backend))?;
        }
    }
    Ok(0)
}

fn run_build_internal(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
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

    let mut moonc_opt = super::get_compiler_flags(source_dir, &cmd.build_flags)?;
    moonc_opt.build_opt.deny_warn = cmd.build_flags.deny_warn;
    let run_mode = RunMode::Build;
    let target_dir = mk_arch_mode_dir(source_dir, target_dir, &moonc_opt, run_mode)?;
    let sort_input = cmd.build_flags.sort_input;

    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.to_path_buf(),
        target_dir,
        sort_input,
        run_mode,
        quiet: cli.quiet,
        verbose: cli.verbose,
        ..Default::default()
    };

    let module = moonutil::scan::scan(
        false,
        &resolved_env,
        &dir_sync_result,
        &moonc_opt,
        &moonbuild_opt,
    )?;
    moonc_opt.build_opt.warn_lists = module
        .packages
        .iter()
        .map(|(name, pkg)| (name.clone(), pkg.warn_list.clone()))
        .collect();
    moonc_opt.build_opt.alert_lists = module
        .packages
        .iter()
        .map(|(name, pkg)| (name.clone(), pkg.alert_list.clone()))
        .collect();

    if cli.dry_run {
        return dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt);
    }

    let trace_flag = cli.trace;
    if trace_flag {
        trace::open("trace.json").context("failed to open `trace.json`")?;
    }

    let result = entry::run_build(&moonc_opt, &moonbuild_opt, &module);
    if trace_flag {
        trace::close();
    }
    result
}
