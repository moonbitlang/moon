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
use moonutil::common::TargetBackend;
use moonutil::common::{MoonbuildOpt, TestOpt};
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::dirs::PackageDirs;
use moonutil::module::ModuleDB;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use moonutil::package::Package;
use n2::trace;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use super::{BuildFlags, UniversalFlags};

/// Test the current package
#[derive(Debug, clap::Parser, Clone)]
pub struct TestSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    /// Run test in the specified package
    #[clap(short, long, num_args(0..))]
    pub package: Option<Vec<String>>,

    /// Run test in the specified file. Only valid when `--package` is also specified.
    #[clap(short, long, requires("package"))]
    pub file: Option<String>,

    /// Run only the index-th test in the file. Only valid when `--file` is also specified.
    #[clap(short, long, requires("file"))]
    pub index: Option<u32>,

    /// Update the test snapshot
    #[clap(short, long)]
    pub update: bool,

    /// Limit of expect test update passes to run, in order to avoid infinite loops
    #[clap(short, long, default_value = "256", requires("update"))]
    pub limit: u32,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Only build, do not run the tests
    #[clap(long)]
    pub build_only: bool,

    /// Run the tests in a target backend sequentially
    #[clap(long)]
    pub no_parallelize: bool,

    /// Print failure message in JSON format
    #[clap(long)]
    pub test_failure_json: bool,

    /// Path to the patch file
    #[clap(long, requires("package"), conflicts_with = "update")]
    pub patch_file: Option<PathBuf>,

    /// Run doc test
    #[clap(long = "doc")]
    pub doc_test: bool,

    /// Run test in markdown file
    #[clap(long = "md", conflicts_with = "doc_test")]
    pub md_test: bool,
}

pub fn run_test(cli: UniversalFlags, cmd: TestSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    if cmd.build_flags.target.is_none() {
        return run_test_internal(&cli, &cmd, &source_dir, &target_dir, None);
    }
    let surface_targets = cmd.build_flags.target.clone().unwrap();
    let targets = lower_surface_targets(&surface_targets);
    if cmd.update && targets.len() > 1 {
        return Err(anyhow::anyhow!("cannot update test on multiple targets"));
    }
    let display_backend_hint = if targets.len() > 1 { Some(()) } else { None };
    let cli = Arc::new(cli);
    let source_dir = Arc::new(source_dir);
    let target_dir = Arc::new(target_dir);
    let mut handles = Vec::new();

    let mut ret_value = 0;
    if cmd.build_flags.serial {
        for t in targets {
            let mut cmd = cmd.clone();
            cmd.build_flags.target_backend = Some(t);
            let x = run_test_internal(&cli, &cmd, &source_dir, &target_dir, display_backend_hint)?;
            ret_value = ret_value.max(x);
        }
    } else {
        for t in targets {
            let cli = Arc::clone(&cli);
            let mut cmd = cmd.clone();
            cmd.build_flags.target_backend = Some(t);
            let source_dir = Arc::clone(&source_dir);
            let target_dir = Arc::clone(&target_dir);

            let handle = thread::spawn(move || {
                run_test_internal(&cli, &cmd, &source_dir, &target_dir, display_backend_hint)
            });

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
    display_backend_hint: Option<()>,
) -> anyhow::Result<i32> {
    // Run moon install before build
    let (resolved_env, dir_sync_result) = auto_sync(
        source_dir,
        &cmd.auto_sync_flags,
        &RegistryConfig::load(),
        cli.quiet,
    )?;

    let run_mode = RunMode::Test;
    let mut moonc_opt = super::get_compiler_flags(source_dir, &cmd.build_flags)?;
    // release is 'false' by default, so we will run test at debug mode(to gain more detailed stack trace info), unless `--release` is specified
    // however, other command like build, check, run, etc, will run at release mode by default
    moonc_opt.build_opt.debug_flag = !cmd.build_flags.release;
    moonc_opt.build_opt.strip_flag = if cmd.build_flags.strip {
        true
    } else if cmd.build_flags.no_strip {
        false
    } else {
        cmd.build_flags.release
    };
    moonc_opt.link_opt.debug_flag = !cmd.build_flags.release;

    // TODO: remove this once LLVM backend is well supported
    if moonc_opt.build_opt.target_backend == TargetBackend::LLVM {
        eprintln!("{}: LLVM backend is experimental and only supported on linux and macos with bleeding moonbit toolchain for now", "Warning".yellow());
    }

    let raw_target_dir = target_dir.to_path_buf();
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

    let patch_file = cmd.patch_file.clone();
    let filter_package = cmd.package.clone().map(|it| it.into_iter().collect());
    let filter_file = &cmd.file;
    let filter_index = cmd.index;
    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.to_path_buf(),
        raw_target_dir,
        target_dir: target_dir.clone(),
        test_opt: Some(TestOpt {
            filter_package: filter_package.clone(),
            filter_file: filter_file.clone(),
            filter_index,
            limit,
            test_failure_json: cmd.test_failure_json,
            display_backend_hint,
            patch_file,
        }),
        check_opt: None,
        build_opt: None,
        sort_input,
        run_mode,
        quiet: true,
        verbose: cli.verbose,
        no_parallelize: cmd.no_parallelize,
        build_graph: cli.build_graph,
        fmt_opt: None,
        args: vec![],
        output_json: false,
        parallelism: cmd.build_flags.jobs,
    };

    let mut module = moonutil::scan::scan(
        false,
        &resolved_env,
        &dir_sync_result,
        &moonc_opt,
        &moonbuild_opt,
    )?;

    let (package_filter, moonbuild_opt) = if let Some(filter_package) = moonbuild_opt
        .test_opt
        .as_ref()
        .and_then(|opt| opt.filter_package.as_ref())
    {
        let all_packages: indexmap::IndexSet<&str> = module
            .get_all_packages()
            .iter()
            .map(|pkg| pkg.0.as_str())
            .collect();

        let mut final_set = indexmap::IndexSet::new();
        for needle in filter_package {
            if all_packages.contains(&needle.as_str()) {
                // exact matching
                final_set.insert(needle.to_string());
            } else {
                let xs = moonutil::fuzzy_match::fuzzy_match(
                    needle.as_str(),
                    all_packages.iter().copied(),
                );
                if let Some(xs) = xs {
                    final_set.extend(xs);
                }
            }
        }

        if let Some(file_filter) = moonbuild_opt
            .test_opt
            .as_ref()
            .and_then(|opt| opt.filter_file.as_ref())
        {
            let find = final_set.iter().any(|pkgname| {
                let pkg = module.get_package_by_name(pkgname);
                let files = pkg.get_all_files();
                files.iter().any(|file| file == file_filter)
            });

            if !find {
                eprintln!(
                    "{}: cannot find file `{}` in package {}, --file only support exact matching",
                    "Warning".yellow(),
                    file_filter,
                    final_set
                        .iter()
                        .map(|p| format!("`{}`", p))
                        .collect::<Vec<String>>()
                        .join(", "),
                );
            }
        };

        let moonbuild_opt = MoonbuildOpt {
            test_opt: Some(TestOpt {
                filter_package: Some(
                    final_set
                        .clone()
                        .into_iter()
                        .map(|x| x.to_string())
                        .collect(),
                ),
                ..moonbuild_opt.test_opt.unwrap()
            }),
            ..moonbuild_opt
        };

        let package_filter = Some(move |pkg: &Package| final_set.contains(&pkg.full_name()));
        (package_filter, moonbuild_opt)
    } else {
        (None, moonbuild_opt)
    };

    for (_, pkg) in module.get_filtered_packages_mut(package_filter) {
        if pkg.is_third_party || pkg.is_main {
            continue;
        }

        pkg.patch_file = cmd.patch_file.clone();

        if cmd.doc_test {
            let pj_path = moonutil::doc_test::gen_doc_test_patch(pkg, &moonc_opt)?;
            pkg.doc_test_patch_file = Some(pj_path);
        }

        if cmd.md_test {
            let pj_path = moonutil::doc_test::gen_md_test_patch(pkg)?;
            pkg.doc_test_patch_file = Some(pj_path.clone());
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

    moonutil::common::set_native_backend_link_flags(
        run_mode,
        cmd.build_flags.release,
        cmd.build_flags.target_backend,
        &mut module,
    )?;

    // add coverage libs if needed
    moonbuild::gen::gen_runtest::add_coverage_to_core_if_needed(&mut module, &moonc_opt)?;

    if cli.dry_run {
        return dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt);
    }

    let res = do_run_test(
        moonc_opt,
        moonbuild_opt,
        build_only,
        auto_update,
        module,
        verbose,
    );

    if cli.trace {
        trace::close();
    }

    res
}

fn do_run_test(
    moonc_opt: MooncOpt,
    moonbuild_opt: MoonbuildOpt,
    build_only: bool,
    auto_update: bool,
    module: ModuleDB,
    verbose: bool,
) -> anyhow::Result<i32> {
    let backend_hint = moonbuild_opt
        .test_opt
        .as_ref()
        .and_then(|opt| opt.display_backend_hint.as_ref())
        .map(|_| format!(" [{}]", moonc_opt.build_opt.target_backend.to_backend_ext()))
        .unwrap_or_default();

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
        "Total tests: {}, passed: {}, failed: {}.{}",
        total,
        passed,
        if failed > 0 {
            failed.to_string().red().to_string()
        } else {
            failed.to_string()
        },
        backend_hint,
    );

    if passed == total {
        Ok(0)
    } else {
        // don't bail! here, use no-zero exit code to indicate test failed
        Ok(2)
    }
}
