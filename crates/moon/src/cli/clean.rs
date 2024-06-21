use anyhow::{bail, Context};
use moonutil::{
    cli::UniversalFlags,
    common::{FileLock, MOON_MOD_JSON},
};

/// Clean the target directory
#[derive(Debug, clap::Parser)]
pub struct CleanSubcommand {}

pub fn run_clean(cli: &UniversalFlags) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("dry-run is not implemented for clean");
    }

    let src_tgt = cli.source_tgt_dir.try_into_package_dirs()?;

    let _lock = FileLock::lock(&src_tgt.target_dir)?;

    if !moonutil::common::check_moon_mod_exists(&src_tgt.source_dir) {
        bail!("could not find `{}`", MOON_MOD_JSON);
    }

    if src_tgt.target_dir.is_dir() {
        std::fs::remove_dir_all(src_tgt.target_dir).context("failed to remove target directory")?;
    }
    Ok(0)
}
