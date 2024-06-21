use anyhow::Context;
use colored::Colorize;
use moonbuild::dry_run;
use moonbuild::entry;
use moonbuild::entry::TestFailedStatus;
use moonbuild::entry::TestResult;
use mooncake::pkg::sync::auto_sync;
use moonutil::common::gen::GeneratedTestDriver;
use moonutil::common::gen::ModuleDB;
use moonutil::common::FileLock;
use moonutil::common::MooncOpt;
use moonutil::common::RunMode;
use moonutil::common::{MoonbuildOpt, TargetBackend, TestOpt};
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::dirs::PackageDirs;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use n2::trace;
use regex::Regex;
use std::path::Path;
use std::path::PathBuf;

use super::{BuildFlags, UniversalFlags};

/// Test the current package
#[derive(Debug, clap::Parser)]
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
}

pub fn run_test(cli: UniversalFlags, cmd: TestSubcommand) -> anyhow::Result<i32> {
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
    // release is 'false' by default, so we will run test at debug mode(to gain more detailed stack trace info), unless `--release` is specified
    // however, other command like build, check, run, etc, will run at release mode by default
    moonc_opt.build_opt.debug_flag = !cmd.release;
    moonc_opt.link_opt.debug_flag = !cmd.release;

    // TODO: remove this when we have a better way to handle this
    if matches!(moonc_opt.link_opt.target_backend, TargetBackend::Js) {
        moonc_opt.extra_build_opt.push("-ryu".into());
        moonc_opt.extra_link_opt.push("-ryu".into());
    }

    let run_mode = RunMode::Test;
    let target_dir = mk_arch_mode_dir(&source_dir, &target_dir, &moonc_opt, run_mode)?;

    if cli.trace {
        trace::open("trace.json").context("failed to open `trace.json`")?;
    }

    let verbose = cli.verbose;
    let build_only = cmd.build_only;
    let auto_update = cmd.update;
    let limit = cmd.limit;
    let sort_input = cmd.build_flags.sort_input;

    let filter_package = cmd.package.map(|it| it.into_iter().collect());
    let filter_file = cmd.file;
    let filter_index = cmd.index;
    let moonbuild_opt = MoonbuildOpt {
        source_dir,
        target_dir: target_dir.clone(),
        test_opt: Some(TestOpt {
            filter_package: filter_package.clone(),
            filter_file: filter_file.clone(),
            filter_index,
        }),
        sort_input,
        run_mode,
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

        // test driver file will be generated via `moon generate-test-driver` command
        let internal_generated_file = target_dir
            .join(pkg.rel.fs_full_name())
            .join("__generated_driver_for_internal_test.mbt");
        pkg.generated_test_drivers
            .push(GeneratedTestDriver::InternalTest(internal_generated_file));

        let underscore_generated_file = target_dir
            .join(pkg.rel.fs_full_name())
            .join("__generated_driver_for_underscore_test.mbt");
        pkg.generated_test_drivers
            .push(GeneratedTestDriver::UnderscoreTest(
                underscore_generated_file,
            ));

        for file in pkg.files.iter().chain(pkg.test_files.iter()) {
            let content = std::fs::read_to_string(file)?;
            let pattern =
                Regex::new(r#"^test[[:blank:]]*("(?P<name>([^\\"]|\\.)*)")?.*$"#).unwrap();

            let filename = file.file_name().unwrap().to_str().unwrap();
            if let Some(ref filter_file) = filter_file {
                if filter_file != filename {
                    continue;
                }
            }

            for line in content.lines() {
                if pattern.captures(line).is_some() {
                    pkg.files_contain_test_block.push(file.clone());
                    break;
                }
            }
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

    let result = do_run_test(
        &moonc_opt,
        &moonbuild_opt,
        build_only,
        auto_update,
        &module,
        verbose,
        limit,
    );
    match result {
        Ok(_) => Ok(0),
        Err(e) => Ok(e.into()),
    }
}

fn do_run_test(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    build_only: bool,
    auto_update: bool,
    module: &ModuleDB,
    verbose: bool,
    limit: u32,
) -> anyhow::Result<TestResult, TestFailedStatus> {
    let result = if !auto_update {
        entry::run_test(
            true,
            moonc_opt,
            moonbuild_opt,
            build_only,
            verbose,
            false,
            module,
        )
    } else {
        let mut result = entry::run_test(
            true,
            moonc_opt,
            moonbuild_opt,
            build_only,
            verbose,
            true,
            module,
        );

        match result {
            Err(TestFailedStatus::ExpectTestFailed(_)) => {
                println!(
                    "\n{}\n",
                    "Auto updating expect tests and retesting ...".bold()
                );

                let (mut should_update, mut count) = (true, 1);
                while should_update && count < limit {
                    result = entry::run_test(
                        true,
                        moonc_opt,
                        moonbuild_opt,
                        build_only,
                        verbose,
                        true,
                        module,
                    );
                    match result {
                        // only continue update when it is a ExpectTestFailed
                        Err(TestFailedStatus::ExpectTestFailed(_)) => {}
                        _ => {
                            should_update = false;
                        }
                    }
                    count += 1;
                }

                result
            }
            _ => result,
        }
    };

    print_test_res(&result);
    result
}

fn print_test_res(test_res: &anyhow::Result<TestResult, TestFailedStatus>) {
    let print = |test_res: &TestResult| {
        let (passed, failed) = (test_res.passed, test_res.failed);
        println!(
            "Total tests: {}, passed: {}, failed: {}.",
            passed + failed,
            if passed > 0 {
                passed.to_string().green()
            } else {
                passed.to_string().normal()
            },
            if failed > 0 {
                failed.to_string().red()
            } else {
                failed.to_string().normal()
            },
        );
    };

    match test_res {
        Ok(test_res) => {
            print(test_res);
        }
        Err(e) => match e {
            TestFailedStatus::ApplyExpectFailed(it) => print(it),
            TestFailedStatus::ExpectTestFailed(it) => print(it),
            TestFailedStatus::Failed(it) => print(it),
            TestFailedStatus::RuntimeError(it) => print(it),
            TestFailedStatus::Others(it) => println!("{}: {:?}", "error".bold().red(), it),
        },
    }
}
