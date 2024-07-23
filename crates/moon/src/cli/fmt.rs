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

use anyhow::bail;
use mooncake::pkg::sync::auto_sync;
use moonutil::{
    common::{FileLock, FmtOpt, MoonbuildOpt, MooncOpt, RunMode},
    dirs::{mk_arch_mode_dir, PackageDirs},
    mooncakes::{sync::AutoSyncFlags, RegistryConfig},
};

use super::UniversalFlags;

/// Format moonbit source code
#[derive(Debug, clap::Parser)]
pub struct FmtSubcommand {
    #[clap(long)]
    check: bool,

    #[clap(long)]
    pub sort_input: bool,
}

pub fn run_fmt(cli: &UniversalFlags, cmd: FmtSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let _lock = FileLock::lock(&target_dir)?;

    let moonc_opt = MooncOpt::default();
    let run_mode = RunMode::Format;
    let target_dir = mk_arch_mode_dir(&source_dir, &target_dir, &moonc_opt, run_mode)?;

    // Resolve dependencies, but don't download anything
    let (resolved_env, dir_sync_result) = auto_sync(
        &source_dir,
        &AutoSyncFlags { frozen: true },
        &RegistryConfig::load(),
        cli.quiet,
    )?;

    let moonbuild_opt = MoonbuildOpt {
        source_dir,
        target_dir: target_dir.clone(),
        sort_input: cmd.sort_input,
        run_mode,
        fmt_opt: Some(FmtOpt { check: cmd.check }),
        ..Default::default()
    };

    let module = moonutil::scan::scan(
        false,
        &resolved_env,
        &dir_sync_result,
        &moonc_opt,
        &moonbuild_opt,
    )?;
    if cli.dry_run {
        bail!("dry-run is not implemented for fmt");
    }
    moonbuild::entry::run_fmt(&module, &moonc_opt, &moonbuild_opt)
}
