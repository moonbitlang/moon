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
use moonbuild::entry;
use moonbuild::watch::watching;
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

use super::pre_build::scan_with_pre_build;
use super::{BuildFlags, UniversalFlags};

/// Build the current package
#[derive(Debug, clap::Parser, Clone)]
pub struct BuildSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Monitor the file system and automatically build artifacts
    #[clap(long, short)]
    pub watch: bool,

    #[clap(long, hide = true)]
    pub show_artifacts: bool,
}

pub fn run_build(cli: &UniversalFlags, cmd: &BuildSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    if cmd.build_flags.target.is_none() {
        return run_build_internal(cli, cmd, &source_dir, &target_dir);
    }
    let surface_targets = cmd.build_flags.target.clone().unwrap();
    let targets = lower_surface_targets(&surface_targets);

    let mut ret_value = 0;
    if cmd.build_flags.serial {
        for t in targets {
            let mut cmd = (*cmd).clone();
            cmd.build_flags.target_backend = Some(t);
            let x = run_build_internal(cli, &cmd, &source_dir, &target_dir)
                .context(format!("failed to run build for target {:?}", t))?;
            ret_value = ret_value.max(x);
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
            let x = handle
                .join()
                .unwrap()
                .context(format!("failed to run build for target {:?}", backend))?;
            ret_value = ret_value.max(x);
        }
    }
    Ok(ret_value)
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

    let raw_target_dir = target_dir;
    let run_mode = RunMode::Build;
    let mut moonc_opt = super::get_compiler_flags(source_dir, &cmd.build_flags)?;
    moonc_opt.build_opt.deny_warn = cmd.build_flags.deny_warn;
    let target_dir = mk_arch_mode_dir(source_dir, target_dir, &moonc_opt, run_mode)?;
    let _lock = FileLock::lock(&target_dir)?;
    let sort_input = cmd.build_flags.sort_input;

    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.to_path_buf(),
        raw_target_dir: raw_target_dir.to_path_buf(),
        target_dir,
        sort_input,
        run_mode,
        quiet: cli.quiet,
        verbose: cli.verbose,
        build_graph: cli.build_graph,
        test_opt: None,
        check_opt: None,
        fmt_opt: None,
        args: vec![],
        output_json: false,
        no_parallelize: false,
    };

    let mut module = scan_with_pre_build(
        false,
        &moonc_opt,
        &moonbuild_opt,
        &resolved_env,
        &dir_sync_result,
    )?;

    moonutil::common::set_native_backend_link_flags(
        run_mode,
        cmd.build_flags.release,
        cmd.build_flags.target_backend,
        &mut module,
    );

    moonc_opt.build_opt.warn_lists = module
        .get_all_packages()
        .iter()
        .map(|(name, pkg)| (name.clone(), pkg.warn_list.clone()))
        .collect();
    moonc_opt.build_opt.alert_lists = module
        .get_all_packages()
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

    let res = if cmd.watch {
        let reg_cfg = RegistryConfig::load();
        watching(
            &moonc_opt,
            &moonbuild_opt,
            &reg_cfg,
            &module,
            raw_target_dir,
        )
    } else {
        entry::run_build(&moonc_opt, &moonbuild_opt, &module)
    };

    if trace_flag {
        trace::close();
    }

    if let (Ok(_), true) = (res.as_ref(), cmd.show_artifacts) {
        // can't use HashMap because the order of the packages is not guaranteed
        // can't use IndexMap because moonc cannot handled ordered map
        let mut artifacts = Vec::new();
        for pkg in module
            .get_topo_pkgs()?
            .iter()
            .filter(|pkg| !pkg.is_third_party)
        {
            let mi = pkg.artifact.with_extension("mi");
            let core = pkg.artifact.with_extension("core");
            artifacts.push((pkg.full_name(), mi, core));
        }
        println!("{}", serde_json::to_string(&artifacts).unwrap());
    }
    res
}
