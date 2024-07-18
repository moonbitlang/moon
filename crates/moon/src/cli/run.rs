// Copyright 2024 International Digital Economy Academy
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
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use anyhow::{bail, Context};
use moonbuild::dry_run;
use moonbuild::entry;
use mooncake::pkg::sync::auto_sync;
use moonutil::common::FileLock;
use moonutil::common::RunMode;
use moonutil::common::SurfaceTarget;
use moonutil::common::{MoonbuildOpt, OutputFormat};
use moonutil::dirs::check_moon_pkg_exist;
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::dirs::PackageDirs;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use n2::trace;

use super::{BuildFlags, UniversalFlags};

/// Run WebAssembly module
#[derive(Debug, clap::Parser)]
pub struct RunSubcommand {
    /// The package to run
    pub package: String,

    #[clap(flatten)]
    pub build_flags: BuildFlags,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    pub args: Vec<String>,
}

pub fn run_run(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    if let Some(SurfaceTarget::All) = cmd.build_flags.target {
        anyhow::bail!("`--target all` is not supported for `run`");
    } else {
        run_run_internal(cli, cmd)
    }
}

pub fn run_run_internal(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
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
    let run_mode = RunMode::Run;
    let target_dir = mk_arch_mode_dir(&source_dir, &target_dir, &moonc_opt, run_mode)?;

    if moonc_opt.link_opt.output_format == OutputFormat::Wat {
        bail!("`--output-wat` is not supported for `run`");
    }

    let sort_input = cmd.build_flags.sort_input;

    let package_path = cmd.package.clone();
    let package = source_dir.join(&package_path);
    if !check_moon_pkg_exist(&package) {
        bail!("{} is not a package", package_path);
    }

    let pkg = moonutil::common::read_package_desc_file_in_dir(&package)?;
    if !pkg.is_main {
        bail!("`{}` is not a main package", package_path);
    }
    let moonbuild_opt = MoonbuildOpt {
        source_dir,
        target_dir,
        sort_input,
        run_mode,
        args: cmd.args,
        quiet: true,
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

    let result = entry::run_run(Some(&package_path), &moonc_opt, &moonbuild_opt, &module);
    if trace_flag {
        trace::close();
    }
    result
}
