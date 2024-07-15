use anyhow::Context;
use moonbuild::check;
use moonbuild::dry_run;
use moonbuild::watcher_is_running;
use moonbuild::{entry, MOON_PID_NAME};
use mooncake::pkg::sync::auto_sync;
use moonutil::cli::UniversalFlags;
use moonutil::common::FileLock;
use moonutil::common::MoonbuildOpt;
use moonutil::common::RunMode;
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::dirs::PackageDirs;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use n2::trace;

use super::{get_compiler_flags, BuildFlags};

/// Check the current package, but don't build object files
#[derive(Debug, clap::Parser)]
pub struct CheckSubcommand {
    /// Monitor the file system and automatically check files
    #[clap(long, short)]
    pub watch: bool,

    #[clap(flatten)]
    pub build_flags: BuildFlags,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,
}

pub fn run_check(cli: &UniversalFlags, cmd: &CheckSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let _lock = FileLock::lock(&target_dir)?;

    // Run moon install before build
    let (resolved_modules, dir_sync_result) = auto_sync(
        &source_dir,
        &cmd.auto_sync_flags,
        &RegistryConfig::load(),
        cli.quiet,
    )?;

    let mut moonc_opt = get_compiler_flags(&source_dir, &cmd.build_flags)?;
    moonc_opt.build_opt.deny_warn = cmd.build_flags.deny_warn;
    let run_mode = RunMode::Check;
    let target_dir = mk_arch_mode_dir(&source_dir, &target_dir, &moonc_opt, run_mode)?;

    let sort_input = cmd.build_flags.sort_input;

    let moonbuild_opt = MoonbuildOpt {
        source_dir,
        target_dir: target_dir.clone(),
        sort_input,
        run_mode,
        quiet: cli.quiet,
        verbose: cli.verbose,
        ..Default::default()
    };

    let module = moonutil::scan::scan(
        false,
        &resolved_modules,
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

    if cli.trace {
        trace::open("trace.json").context("failed to open `trace.json`")?;
    }

    let watch_mode = cmd.watch;

    if watch_mode {
        let reg_cfg = RegistryConfig::load();
        check::watch::watch_single_thread(&moonc_opt, &moonbuild_opt, &reg_cfg, &module)
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
            let result = entry::run_check(&moonc_opt, &moonbuild_opt, &module);
            if cli.trace {
                trace::close();
            }
            result
        }
    }
}
