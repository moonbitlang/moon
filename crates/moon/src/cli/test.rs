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
use colored::Colorize;
use moonbuild::dry_run;
use moonbuild::entry;
use mooncake::pkg::sync::auto_sync;
use moonutil::common::lower_surface_targets;
use moonutil::common::FileLock;
use moonutil::common::GeneratedTestDriver;
use moonutil::common::MooncOpt;
use moonutil::common::RunMode;
use moonutil::common::{MoonbuildOpt, TestOpt};
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::dirs::PackageDirs;
use moonutil::module::ModuleDB;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use n2::trace;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use super::{BuildFlags, UniversalFlags};

/// Test the current package
#[derive(Debug, clap::Parser, Clone)]
pub struct TestSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    /// run test at release compiled mode
    #[clap(long)]
    pub release: bool,

    /// Run test in the specified package
    #[clap(short, long, num_args(0..))]
    pub package: Option<Vec<PathBuf>>,

    /// Run test in the specified file. Only valid when `--package` is also specified.
    #[clap(short, long, requires("package"))]
    pub file: Option<String>,

    /// Run only the index-th test in the file. Only valid when `--file` is also specified.
    #[clap(short, long, requires("file"))]
    pub index: Option<u32>,

    /// Only build, do not run the tests
    #[clap(long)]
    pub build_only: bool,

    /// Update the test snapshot
    #[clap(short, long)]
    pub update: bool,

    /// Limit of expect test update passes to run, in order to avoid infinite loops
    #[clap(short, long, default_value = "256", requires("update"))]
    pub limit: u32,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    #[clap(long)]
    pub no_parallelize: bool,

    #[clap(long)]
    pub test_failure_json: bool,
}

pub fn run_test(cli: UniversalFlags, cmd: TestSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    if cmd.build_flags.target.is_none() {
        return run_test_internal(&cli, &cmd, &source_dir, &target_dir);
    }
    let surface_targets = cmd.build_flags.target.clone().unwrap();
    let targets = lower_surface_targets(&surface_targets);
    if cmd.update && targets.len() > 1 {
        return Err(anyhow::anyhow!("cannot update test on multiple targets"));
    }
    let cli = Arc::new(cli);
    let source_dir = Arc::new(source_dir);
    let target_dir = Arc::new(target_dir);
    let mut handles = Vec::new();

    let mut ret_value = 0;
    if cmd.build_flags.serial {
        for t in targets {
            let mut cmd = cmd.clone();
            cmd.build_flags.target_backend = Some(t);
            let x = run_test_internal(&cli, &cmd, &source_dir, &target_dir)?;
            ret_value = ret_value.max(x);
        }
    } else {
        for t in targets {
            let cli = Arc::clone(&cli);
            let mut cmd = cmd.clone();
            cmd.build_flags.target_backend = Some(t);
            let source_dir = Arc::clone(&source_dir);
            let target_dir = Arc::clone(&target_dir);

            let handle =
                thread::spawn(move || run_test_internal(&cli, &cmd, &source_dir, &target_dir));

            handles.push((t, handle));
        }

        for (backend, handle) in handles {
            let x = handle
                .join()
                .unwrap()
                .context(format!("failed to run test for target {:?}", backend))?;
            ret_value = ret_value.max(x);
        }
    }
    Ok(ret_value)
}

fn run_test_internal(
    cli: &UniversalFlags,
    cmd: &TestSubcommand,
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
    // release is 'false' by default, so we will run test at debug mode(to gain more detailed stack trace info), unless `--release` is specified
    // however, other command like build, check, run, etc, will run at release mode by default
    moonc_opt.build_opt.debug_flag = !cmd.release;
    moonc_opt.link_opt.debug_flag = !cmd.release;

    let run_mode = RunMode::Test;
    let target_dir = mk_arch_mode_dir(source_dir, target_dir, &moonc_opt, run_mode)?;
    let _lock = FileLock::lock(&target_dir)?;

    if cli.trace {
        trace::open("trace.json").context("failed to open `trace.json`")?;
    }

    let verbose = cli.verbose;
    let build_only = cmd.build_only;
    let auto_update = cmd.update;
    let limit = cmd.limit;
    let sort_input = cmd.build_flags.sort_input;

    let filter_package = cmd.package.clone().map(|it| it.into_iter().collect());
    let filter_file = &cmd.file;
    let filter_index = cmd.index;
    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.to_path_buf(),
        target_dir: target_dir.clone(),
        test_opt: Some(TestOpt {
            filter_package: filter_package.clone(),
            filter_file: filter_file.clone(),
            filter_index,
            limit,
            test_failure_json: cmd.test_failure_json,
        }),
        sort_input,
        run_mode,
        quiet: true,
        verbose: cli.verbose,
        no_parallelize: cmd.no_parallelize,
        ..Default::default()
    };

    let mut module = moonutil::scan::scan(
        false,
        &resolved_env,
        &dir_sync_result,
        &moonc_opt,
        &moonbuild_opt,
    )?;

    for (pkgname, pkg) in module.packages.iter_mut() {
        if let Some(ref package) = filter_package {
            if !package.contains(Path::new(pkgname)) {
                continue;
            }
        }

        if pkg.is_third_party || pkg.is_main {
            continue;
        }

        {
            // test driver file will be generated via `moon generate-test-driver` command
            let internal_generated_file = target_dir
                .join(pkg.rel.fs_full_name())
                .join("__generated_driver_for_internal_test.mbt");
            pkg.generated_test_drivers
                .push(GeneratedTestDriver::InternalTest(internal_generated_file));

            let whitebox_generated_file = target_dir
                .join(pkg.rel.fs_full_name())
                .join("__generated_driver_for_whitebox_test.mbt");
            pkg.generated_test_drivers
                .push(GeneratedTestDriver::WhiteboxTest(whitebox_generated_file));

            let blackbox_generated_file = target_dir
                .join(pkg.rel.fs_full_name())
                .join("__generated_driver_for_blackbox_test.mbt");
            pkg.generated_test_drivers
                .push(GeneratedTestDriver::BlackboxTest(blackbox_generated_file));
        }
    }

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
        return dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt).map(From::from);
    }

    if cli.trace {
        trace::close();
    }

    do_run_test(
        &moonc_opt,
        &moonbuild_opt,
        build_only,
        auto_update,
        &module,
        verbose,
    )
}

fn do_run_test(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    build_only: bool,
    auto_update: bool,
    module: &ModuleDB,
    verbose: bool,
) -> anyhow::Result<i32> {
    let test_res = entry::run_test(
        moonc_opt,
        moonbuild_opt,
        build_only,
        verbose,
        auto_update,
        module,
    )?;

    // don't print test summary if build_only
    if build_only {
        return Ok(0);
    }

    let total = test_res.len();
    let passed = test_res.iter().filter(|r| r.is_ok()).count();

    let failed = total - passed;
    println!(
        "Total tests: {}, passed: {}, failed: {}.",
        if total > 0 {
            total.to_string().blue().to_string()
        } else {
            total.to_string()
        },
        if passed > 0 {
            passed.to_string().green().to_string()
        } else {
            passed.to_string()
        },
        if failed > 0 {
            failed.to_string().red().to_string()
        } else {
            failed.to_string()
        }
    );

    if passed == total {
        Ok(0)
    } else {
        // don't bail! here, use no-zero exit code to indicate test failed
        Ok(2)
    }
}
