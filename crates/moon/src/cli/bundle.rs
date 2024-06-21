use moonbuild::dry_run;
use mooncake::pkg::sync::auto_sync;
use moonutil::{
    cli::UniversalFlags,
    common::{FileLock, MoonbuildOpt, RunMode, TargetBackend},
    dirs::{mk_arch_mode_dir, PackageDirs},
    mooncakes::{sync::AutoSyncFlags, RegistryConfig},
};

use super::BuildFlags;

/// Bundle the module
#[derive(Debug, clap::Parser)]
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

    let _lock = FileLock::lock(&target_dir)?;

    // Run moon install before build
    let (resolved_env, dir_sync_result) = auto_sync(
        &source_dir,
        &cmd.auto_sync_flags,
        &RegistryConfig::load(),
        cli.quiet,
    )?;

    let mut moonc_opt = super::get_compiler_flags(&source_dir, &cmd.build_flags)?;
    let run_mode = RunMode::Bundle;
    let sort_input = cmd.build_flags.sort_input;

    if cmd.all {
        for target in [
            TargetBackend::Wasm,
            TargetBackend::WasmGC,
            TargetBackend::Js,
        ] {
            let mut moonc_opt = moonc_opt.clone();
            moonc_opt.build_opt.target_backend = target;
            moonc_opt.link_opt.target_backend = target;
            let target_dir = mk_arch_mode_dir(&source_dir, &target_dir, &moonc_opt, run_mode)?;

            let moonbuild_opt = MoonbuildOpt {
                source_dir: source_dir.clone(),
                target_dir,
                sort_input,
                run_mode,
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
                dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt)?;
            } else {
                moonbuild::entry::run_bundle(&module, &moonbuild_opt, &moonc_opt, false)?;
            }
        }
        Ok(0)
    } else {
        let target_dir = mk_arch_mode_dir(&source_dir, &target_dir, &moonc_opt, run_mode)?;

        let moonbuild_opt = MoonbuildOpt {
            source_dir,
            target_dir,
            sort_input,
            run_mode,
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
        moonbuild::entry::run_bundle(&module, &moonbuild_opt, &moonc_opt, false)
    }
}
