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
use anyhow::bail;
use colored::Colorize;
use indexmap::IndexMap;
use log::warn;
use moonbuild::dry_run;
use moonbuild::entry;
use moonbuild_rupes_recta::build_plan::InputDirective;
use moonbuild_rupes_recta::intent::UserIntent;
use moonbuild_rupes_recta::model::BuildPlanNode;
use moonbuild_rupes_recta::model::BuildTarget;
use moonbuild_rupes_recta::model::PackageId;
use mooncake::pkg::sync::auto_sync;
use mooncake::pkg::sync::auto_sync_for_single_mbt_md;
use moonutil::common::PrePostBuild;
use moonutil::common::{BLACKBOX_TEST_DRIVER, DOT_MBT_DOT_MD, SINGLE_FILE_TEST_PACKAGE};
use moonutil::common::{
    FileLock, GeneratedTestDriver, MOONBITLANG_CORE, MbtMdHeader, MoonbuildOpt, MooncOpt,
    OutputFormat, RunMode, TargetBackend, TestOpt, lower_surface_targets,
    parse_front_matter_config,
};
use moonutil::cond_expr::CompileCondition;
use moonutil::cond_expr::OptLevel;
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::module::ModuleDB;
use moonutil::mooncakes::RegistryConfig;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::package::Package;
use moonutil::path::PathComponent;
use n2::trace;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{Level, instrument};

use crate::cli::pre_build::scan_with_x_build;
use crate::rr_build;
use crate::rr_build::preconfig_compile;
use crate::rr_build::{BuildConfig, CalcUserIntentOutput};
use crate::run::TestFilter;
use crate::run::TestIndex;
use crate::run::perform_promotion;

use super::BenchSubcommand;
use super::{BuildFlags, UniversalFlags};

/// Print test summary statistics in the legacy format
fn print_test_summary(total: usize, passed: usize, quiet: bool, backend_hint: Option<&str>) {
    if total == 0 {
        eprintln!("{}: no test entry found.", "Warning".yellow().bold());
    }

    let failed = total - passed;
    let has_failures = failed > 0;

    if !quiet || has_failures {
        let backend_suffix = backend_hint
            .map(|hint| format!(" [{}]", hint))
            .unwrap_or_default();

        println!(
            "Total tests: {}, passed: {}, failed: {}.{}",
            total,
            passed,
            if has_failures {
                failed.to_string().red().to_string()
            } else {
                failed.to_string()
            },
            backend_suffix,
        );
    }
}

/// Test the current package
#[derive(Debug, clap::Parser, Clone)]
pub struct TestSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    /// Run test in the specified package
    #[clap(short, long, num_args(1..))]
    pub package: Option<Vec<String>>,

    /// Run test in the specified file. Only valid when `--package` is also specified.
    #[clap(short, long)]
    pub file: Option<String>,

    /// Run only the index-th test in the file. Only valid when `--file` is also specified.
    #[clap(short, long)]
    pub index: Option<u32>,

    /// Run only the index-th doc test in the file. Only valid when `--file` is also specified.
    #[clap(long, conflicts_with = "index")]
    pub doc_index: Option<u32>,

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

    /// Run test in single file or directory. If in a project, runs only this
    /// package (if matches a package path) or file (if matches a file in
    /// package); otherwise, runs in a temporary project.
    #[clap(conflicts_with_all = ["file", "package"], name="PATH")]
    pub single_file: Option<PathBuf>,
}

#[instrument(skip_all)]
pub fn run_test(cli: UniversalFlags, cmd: TestSubcommand) -> anyhow::Result<i32> {
    // Check if we're running within a project
    let dirs = match cli.source_tgt_dir.try_into_package_dirs() {
        Ok(dirs) => dirs,
        Err(e @ moonutil::dirs::PackageDirsError::NotInProject(_)) => {
            // Now we're talking about real single-file scenario.
            if cmd.single_file.is_some() {
                return run_test_in_single_file(&cli, &cmd);
            } else {
                return Err(e.into());
            }
        }
        Err(e) => {
            return Err(e.into());
        }
    };

    if cmd.doc_test {
        eprintln!(
            "{}: --doc flag is deprecated and will be removed in the future, please use `moon test` directly",
            "Warning".yellow(),
        );
    }

    let Some(surface_targets) = &cmd.build_flags.target else {
        return run_test_internal(&cli, &cmd, &dirs.source_dir, &dirs.target_dir, None);
    };
    let targets = lower_surface_targets(surface_targets);
    if cmd.update && targets.len() > 1 {
        return Err(anyhow::anyhow!("cannot update test on multiple targets"));
    }
    let display_backend_hint = if targets.len() > 1 { Some(()) } else { None };

    let mut ret_value = 0;
    for t in targets {
        let mut cmd = cmd.clone();
        cmd.build_flags.target_backend = Some(t);
        let x = run_test_internal(
            &cli,
            &cmd,
            &dirs.source_dir,
            &dirs.target_dir,
            display_backend_hint,
        )
        .context(format!("failed to run test for target {t:?}"))?;
        ret_value = ret_value.max(x);
    }
    Ok(ret_value)
}

#[instrument(skip_all)]
fn run_test_internal(
    cli: &UniversalFlags,
    cmd: &TestSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    display_backend_hint: Option<()>,
) -> anyhow::Result<i32> {
    run_test_or_bench_internal(
        cli,
        cmd.into(),
        source_dir,
        target_dir,
        display_backend_hint,
    )
}

#[instrument(level = Level::DEBUG, skip_all)]
fn run_test_in_single_file(cli: &UniversalFlags, cmd: &TestSubcommand) -> anyhow::Result<i32> {
    let single_file_path = &dunce::canonicalize(cmd.single_file.as_ref().unwrap()).unwrap();
    let source_dir = single_file_path.parent().unwrap().to_path_buf();
    let raw_target_dir = source_dir.join("target");

    let mbt_md_header = parse_front_matter_config(single_file_path)?;
    let target_backend = if let Some(moonutil::common::MbtMdHeader {
        moonbit:
            Some(moonutil::common::MbtMdSection {
                backend: Some(backend),
                ..
            }),
    }) = &mbt_md_header
    {
        TargetBackend::str_to_backend(backend)?
    } else {
        cmd.build_flags
            .target_backend
            .unwrap_or(TargetBackend::WasmGC)
    };

    let debug_flag = !cmd.build_flags.release;

    let target_dir = raw_target_dir
        .join(target_backend.to_dir_name())
        .join(if debug_flag { "debug" } else { "release" })
        .join(RunMode::Test.to_dir_name());

    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.clone(),
        target_dir: target_dir.clone(),
        raw_target_dir: raw_target_dir.clone(),
        test_opt: Some(TestOpt {
            filter_package: Some(HashSet::from([SINGLE_FILE_TEST_PACKAGE.to_string()])),
            filter_file: cmd.file.clone(),
            filter_index: cmd.index,
            filter_doc_index: cmd.doc_index,
            limit: 256,
            test_failure_json: false,
            display_backend_hint: None,
            patch_file: None,
        }),
        check_opt: None,
        build_opt: None,
        sort_input: cmd.build_flags.sort_input,
        run_mode: RunMode::Test,
        quiet: true,
        verbose: cli.verbose,
        no_parallelize: cmd.no_parallelize,
        build_graph: cli.build_graph,
        fmt_opt: None,
        args: vec![],
        output_json: false,
        parallelism: cmd.build_flags.jobs,
        use_tcc_run: false,
        dynamic_stub_libs: None,
        render_no_loc: cmd.build_flags.render_no_loc,
    };
    let moonc_opt = MooncOpt {
        build_opt: moonutil::common::BuildPackageFlags {
            debug_flag,
            strip_flag: false,
            source_map: debug_flag,
            enable_coverage: false,
            deny_warn: false,
            target_backend,
            warn_list: cmd.build_flags.warn_list.clone(),
            alert_list: cmd.build_flags.alert_list.clone(),
            enable_value_tracing: cmd.build_flags.enable_value_tracing,
        },
        link_opt: moonutil::common::LinkCoreFlags {
            debug_flag,
            source_map: debug_flag,
            output_format: match target_backend {
                TargetBackend::Js => OutputFormat::Js,
                TargetBackend::Native => OutputFormat::Native,
                TargetBackend::LLVM => OutputFormat::LLVM,
                _ => OutputFormat::Wasm,
            },
            target_backend,
        },
        extra_build_opt: vec![],
        extra_link_opt: vec![],
        nostd: false,
        render: !cmd.build_flags.no_render,
        single_file: true,
    };
    let module =
        get_module_for_single_file(single_file_path, &moonc_opt, &moonbuild_opt, mbt_md_header)?;

    if cli.dry_run {
        return dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt);
    }

    do_run_test(
        moonc_opt,
        moonbuild_opt,
        cmd.build_only,
        cmd.update,
        module,
        cli.verbose,
        cli.quiet,
    )
}

pub fn get_module_for_single_file(
    single_file_path: &Path,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    front_matter_config: Option<MbtMdHeader>,
) -> anyhow::Result<ModuleDB> {
    let gen_single_file_pkg = |moonc_opt: &MooncOpt, single_file_path: &Path| -> Package {
        let path_comp = PathComponent {
            components: vec!["moon".to_string(), "test".to_string()],
        };
        let pkg_rel_name = "single";

        let single_file_string = single_file_path.display().to_string();
        let source_dir = single_file_path.parent().unwrap().to_path_buf();
        let target_dir = &moonbuild_opt.target_dir;

        Package {
            is_main: false,
            force_link: false,
            is_third_party: false,
            root_path: source_dir.clone(),
            module_root: Arc::from(source_dir),
            root: path_comp,
            rel: PathComponent {
                components: vec![pkg_rel_name.to_string()],
            },
            files: IndexMap::new(),
            wbtest_files: IndexMap::new(),
            test_files: if single_file_string.ends_with(".mbt") {
                IndexMap::from([(single_file_path.to_path_buf(), CompileCondition::default())])
            } else {
                IndexMap::new()
            },
            mbt_md_files: if single_file_string.ends_with(DOT_MBT_DOT_MD) {
                IndexMap::from([(single_file_path.to_path_buf(), CompileCondition::default())])
            } else {
                IndexMap::new()
            },
            files_contain_test_block: vec![single_file_path.to_path_buf()],
            with_sub_package: None,
            is_sub_package: false,
            imports: vec![],
            wbtest_imports: vec![],
            test_imports: vec![],
            generated_test_drivers: vec![GeneratedTestDriver::BlackboxTest(
                target_dir.join(pkg_rel_name).join(BLACKBOX_TEST_DRIVER),
            )],
            artifact: target_dir
                .join(pkg_rel_name)
                .join(format!("{pkg_rel_name}.core")),
            link: None,
            warn_list: moonc_opt.build_opt.warn_list.clone(),
            alert_list: moonc_opt.build_opt.alert_list.clone(),
            targets: None,
            pre_build: None,
            patch_file: None,
            no_mi: false,
            install_path: None,
            bin_name: None,
            bin_target: moonc_opt.link_opt.target_backend,
            enable_value_tracing: moonc_opt.build_opt.enable_value_tracing,
            supported_targets: HashSet::from_iter([moonc_opt.link_opt.target_backend]),
            stub_lib: None,
            virtual_pkg: None,
            virtual_mbti_file: None,
            implement: None,
            overrides: None,
            link_flags: None,
            link_libs: vec![],
            link_search_paths: vec![],
        }
    };

    let (resolved_env, dir_sync_result, moon_mod) =
        auto_sync_for_single_mbt_md(moonc_opt, moonbuild_opt, front_matter_config)?;

    let mut module = moonutil::scan::scan(
        false,
        Some(moon_mod),
        &resolved_env,
        &dir_sync_result,
        moonc_opt,
        moonbuild_opt,
    )?;

    let mut package = gen_single_file_pkg(moonc_opt, single_file_path);
    let imports = module
        .get_all_packages()
        .iter()
        .map(|(_, pkg)| moonutil::path::ImportComponent {
            path: moonutil::path::ImportPath {
                module_name: pkg.root.to_string(),
                rel_path: pkg.rel.clone(),
                is_3rd: true,
            },
            alias: None,
            sub_package: pkg.is_sub_package,
        })
        // we put "moonbitlang/core/abort" in ModuleDB.packages in scan step, it's logical, so we need to filter it out
        .filter(|import| import.path.module_name != MOONBITLANG_CORE)
        .collect::<Vec<_>>();
    package.imports = imports;

    let packages = module.get_all_packages_mut();
    packages.insert(package.full_name(), package.clone());

    let mut graph = petgraph::graph::DiGraph::new();
    for (_, pkg) in packages.iter() {
        graph.add_node(pkg.full_name());
    }
    module.graph = graph;

    // for native backend
    let _ = moonutil::common::set_native_backend_link_flags(
        moonbuild_opt.run_mode,
        moonc_opt.build_opt.target_backend,
        &mut module,
    )?;

    Ok(module)
}

pub(crate) struct TestLikeSubcommand<'a> {
    pub run_mode: RunMode,
    pub build_flags: &'a BuildFlags,
    /// An explicit file filter -- for when you write `moon test <file>` in a project.
    ///
    /// This should behave similar to `moon run <file>`. This should act like
    /// both a package filter and optional file filter.
    ///
    /// FIXME: This is a reuse of the single-file input pattern. Will need a full overhaul.
    pub explicit_file_filter: Option<&'a Path>,
    pub package: &'a Option<Vec<String>>,
    pub file: &'a Option<String>,
    pub index: &'a Option<u32>,
    pub doc_index: &'a Option<u32>,
    pub update: bool,
    pub limit: u32,
    pub auto_sync_flags: &'a AutoSyncFlags,
    pub build_only: bool,
    pub no_parallelize: bool,
    pub test_failure_json: bool,
    pub patch_file: &'a Option<PathBuf>,
}

impl<'a> From<&'a TestSubcommand> for TestLikeSubcommand<'a> {
    fn from(cmd: &'a TestSubcommand) -> Self {
        Self {
            run_mode: RunMode::Test,
            build_flags: &cmd.build_flags,
            package: &cmd.package,
            explicit_file_filter: cmd.single_file.as_deref(),
            file: &cmd.file,
            index: &cmd.index,
            doc_index: &cmd.doc_index,
            update: cmd.update,
            limit: cmd.limit,
            auto_sync_flags: &cmd.auto_sync_flags,
            build_only: cmd.build_only,
            no_parallelize: cmd.no_parallelize,
            test_failure_json: cmd.test_failure_json,
            patch_file: &cmd.patch_file,
        }
    }
}
impl<'a> From<&'a BenchSubcommand> for TestLikeSubcommand<'a> {
    fn from(cmd: &'a BenchSubcommand) -> Self {
        Self {
            run_mode: RunMode::Bench,
            build_flags: &cmd.build_flags,
            explicit_file_filter: None,
            package: &cmd.package,
            file: &cmd.file,
            index: &cmd.index,
            doc_index: &None,
            update: false,
            limit: 256, // FIXME: unsure about why this default, shouldn't bench have only 1 run?
            auto_sync_flags: &cmd.auto_sync_flags,
            build_only: cmd.build_only,
            no_parallelize: cmd.no_parallelize,
            test_failure_json: false,
            patch_file: &None,
        }
    }
}

#[instrument(skip_all)]
pub(crate) fn run_test_or_bench_internal(
    cli: &UniversalFlags,
    cmd: TestLikeSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    display_backend_hint: Option<()>,
) -> anyhow::Result<i32> {
    // Accept -i/--doc-index when the positional PATH refers to a file; otherwise they require --file.
    // explicit_is_file is true only when PATH is an existing regular file.
    let explicit_is_file = cmd.explicit_file_filter.is_some_and(|p| p.is_file());

    if cmd.package.is_none() && cmd.file.is_some() {
        anyhow::bail!("`--file` must be used with `--package`");
    }
    if cmd.file.is_none() && cmd.index.is_some() && !explicit_is_file {
        anyhow::bail!("`--index` must be used with `--file`");
    }
    if cmd.file.is_none() && cmd.doc_index.is_some() && !explicit_is_file {
        anyhow::bail!("`--doc-index` must be used with `--file`");
    }
    if cmd.explicit_file_filter.is_some() && (cmd.package.is_some() || cmd.file.is_some()) {
        anyhow::bail!("cannot filter package or files when testing a single file in a project");
    }

    if cli.unstable_feature.rupes_recta {
        run_test_rr(cli, &cmd, source_dir, target_dir, display_backend_hint)
    } else {
        run_test_or_bench_internal_legacy(cli, cmd, source_dir, target_dir, display_backend_hint)
    }
}

#[instrument(skip_all)]
fn run_test_rr(
    cli: &UniversalFlags,
    cmd: &TestLikeSubcommand<'_>,
    source_dir: &Path,
    target_dir: &Path,
    display_backend_hint: Option<()>, // FIXME: unsure why it's option but as-is for now
) -> Result<i32, anyhow::Error> {
    let is_bench = cmd.run_mode == RunMode::Bench;
    let default_opt_level = if is_bench {
        OptLevel::Release
    } else {
        OptLevel::Debug
    };
    let preconfig = preconfig_compile(
        cmd.auto_sync_flags,
        cli,
        cmd.build_flags,
        target_dir,
        default_opt_level,
        RunMode::Test,
    );

    let mut filter = TestFilter::default();
    let (build_meta, build_graph) = rr_build::plan_build(
        preconfig,
        &cli.unstable_feature,
        source_dir,
        target_dir,
        Box::new(|resolved, main_modules| {
            calc_user_intent(resolved, main_modules, cmd, &mut filter)
        }),
    )?;

    if cli.dry_run {
        rr_build::print_dry_run(
            &build_graph,
            build_meta.artifacts.values(),
            source_dir,
            target_dir,
        );
        // The legacy behavior does not print the test commands, so we skip it too.

        Ok(0)
    } else {
        let _lock = FileLock::lock(target_dir)?;

        let build_config = BuildConfig::from_flags(cmd.build_flags, &cli.unstable_feature);

        // since n2 build consumes the graph, we back it up for reruns
        let build_graph_backup = cmd.update.then(|| build_graph.clone());
        let result = rr_build::execute_build(&build_config, build_graph, target_dir)?;

        if !result.successful() || cmd.build_only {
            return Ok(result.return_code_for_success());
        }

        let mut test_result = crate::run::run_tests(&build_meta, target_dir, &filter)?;

        let backend_hint = display_backend_hint
            .and(cmd.build_flags.target_backend)
            .map(|t| t.to_backend_ext());

        if cmd.update {
            let mut loop_count = 0;
            let mut last_test_result = None;
            loop {
                // Promote test results
                let promotion_source = last_test_result.as_ref().unwrap_or(&test_result);
                let (rerun_count, rerun_filter) =
                    perform_promotion(promotion_source).expect("Failed to promote tests");
                if rerun_filter.is_empty() {
                    break; // Nothing to promote
                }

                // Apply loop count limits
                if loop_count >= cmd.limit {
                    warn!(
                        "reached the limit of {} update passes, stopping further updates.",
                        cmd.limit
                    );
                    break;
                }
                loop_count += 1;

                warn!("Updated {rerun_count} snapshots and retesting...");

                // Get the graph from backup
                let build_graph = build_graph_backup
                    .as_ref()
                    .cloned()
                    .expect("build graph backup should be present when update is true");

                // Calculate which files to rebuild
                let want_files = rerun_filter
                    .0
                    .keys()
                    .cloned() // All targets to rerun
                    .flat_map(node_from_target) // converted to nodes
                    .flat_map(|node| {
                        // their artifacts
                        build_meta
                            .artifacts
                            .get(&node)
                            .expect("test node from the last test run should have artifact")
                            .artifacts
                            .as_slice()
                    });

                // Run the build
                let result = rr_build::execute_build_partial(
                    &build_config,
                    build_graph,
                    target_dir,
                    Box::new(|work| {
                        for file_path in want_files {
                            let file_path_str = file_path.to_string_lossy();
                            let file = work
                                .lookup(&file_path_str)
                                .expect("File should exist in work");
                            work.want_file(file).context("Failed to want file")?;
                        }
                        Ok(())
                    }),
                )?;

                if !result.successful() {
                    return Ok(result.return_code_for_success());
                }

                // Run the tests
                let rerun_filter = TestFilter {
                    filter: Some(rerun_filter),
                };
                let new_test_result =
                    crate::run::run_tests(&build_meta, target_dir, &rerun_filter)?;

                // Merge test results
                test_result.merge(&new_test_result);
                last_test_result = Some(new_test_result);
            }
        }

        test_result.print_result(&build_meta, cli.verbose);
        let summary = test_result.summary();
        print_test_summary(summary.total, summary.passed, cli.quiet, backend_hint);

        if summary.total == summary.passed {
            Ok(0)
        } else {
            Ok(1)
        }
    }
}

/// The nodes wanted to run a test for a build target
fn node_from_target(x: BuildTarget) -> [BuildPlanNode; 2] {
    [
        BuildPlanNode::make_executable(x),
        BuildPlanNode::generate_test_info(x),
    ]
}

/// Apply explicit PATH filter (acts as package and optional file filter).
/// `test_index` selects a single test (regular/doc) when PATH is a file.
fn apply_explicit_file_filter(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    out_filter: &mut TestFilter,
    file_filter: &Path,
    test_index: Option<TestIndex>,
) -> Result<(), anyhow::Error> {
    let input_path = dunce::canonicalize(file_filter).with_context(|| {
        format!(
            "failed to canonicalize the specified file path: {}",
            file_filter.display()
        )
    })?;
    let input_path_parent = input_path.parent();
    let input_filename = input_path.file_name();

    // TODO: known issue: if a path refers to a dir and its parent is a package
    // and itself is not, the parent will be used as the package filter.
    let mut found_path = None;
    let mut found_path_parent = None;
    for m in resolve_output.local_modules() {
        for p in resolve_output
            .pkg_dirs
            .packages_for_module(*m)
            .expect("Module should exist")
            .values()
        {
            let pkg = resolve_output.pkg_dirs.get_package(*p);
            if pkg.root_path == input_path {
                found_path = Some(p);
            } else if let Some(parent) = input_path_parent
                && pkg.root_path == parent
            {
                found_path_parent = Some(p);
            }
        }
    }

    // Prefer exact match, otherwise parent match
    let (pkg, file) = if let Some(pkg) = found_path {
        (pkg, None)
    } else if let (Some(pkg), Some(filename)) = (found_path_parent, input_filename) {
        (pkg, Some(filename))
    } else if let (Some(_), None) = (found_path_parent, input_filename) {
        unreachable!("For a normalized path, if it has a parent, it should also have a filename");
    } else {
        bail!(
            "cannot find a package matching the specified file path: {}",
            file_filter.display()
        );
    };
    out_filter.add_autodetermine_target(
        *pkg,
        file.map(|x| x.to_string_lossy()).as_deref(),
        test_index,
    );
    Ok(())
}

/// Apply the hierarchy of filters of packages, file and index
#[allow(clippy::too_many_arguments)]
fn apply_list_of_filters(
    affected_packages: impl Iterator<Item = PackageId>,
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    package_filter: &[String],
    file_filter: Option<&str>,
    index_filter: Option<u32>,
    doc_index_filter: Option<u32>,
    patch_file: Option<&Path>,
    out_filter: &mut TestFilter,
) -> Result<InputDirective, anyhow::Error> {
    // We deliberately didn't allow a string to be parsed into a package
    // name, because different package may have a same name. However, in a
    // module the package name should be unique, so we can make a map here.
    let name_map = affected_packages
        .map(|id| {
            let name = resolve_output.pkg_dirs.get_package(id).fqn.to_string();
            (name, id)
        })
        .collect::<HashMap<_, _>>();

    // Fuzzy match a string from the map
    let fuzzy_names = |s: &str| -> SmallVec<[PackageId; 1]> {
        if let Some(&id) = name_map.get(s) {
            SmallVec::from_buf([id])
        } else {
            let all_names = name_map.keys().map(|k| k.as_str());
            let xs = moonutil::fuzzy_match::fuzzy_match(s, all_names);
            if let Some(xs) = xs {
                xs.into_iter()
                    .filter_map(|name| name_map.get(&name).copied())
                    .collect()
            } else {
                warn!("no package found matching test filter `{}`", s);
                SmallVec::new()
            }
        }
    };

    let mut filtered_package_ids = SmallVec::<[PackageId; 1]>::new();

    // Collect all the package ids that match the filter
    for p in package_filter {
        let names = fuzzy_names(p);
        if names.is_empty() {
            warn!("no package found matching filter `{}`", p);
        }
        filtered_package_ids.extend_from_slice(&names);
    }

    // Calculate resulting filter & target list
    let mut input_directive = InputDirective::default();
    #[allow(clippy::comparison_chain)]
    if filtered_package_ids.len() == 1 {
        // Single filtered package, can apply file/index filtering
        let pkg_id = filtered_package_ids[0];
        if let Some(id) = index_filter {
            out_filter.add_autodetermine_target(pkg_id, file_filter, Some(TestIndex::Regular(id)));
        } else if let Some(id) = doc_index_filter {
            out_filter.add_autodetermine_target(pkg_id, file_filter, Some(TestIndex::DocTest(id)));
        } else {
            out_filter.add_autodetermine_target(pkg_id, file_filter, None);
        }

        input_directive = rr_build::build_patch_directive_for_package(pkg_id, false, patch_file)
            .context("failed to build input directive")?;
    } else if filtered_package_ids.len() > 1 {
        let package_names = || {
            filtered_package_ids
                .iter()
                .map(|id| resolve_output.pkg_dirs.get_package(*id).fqn.to_string())
                .collect::<Vec<_>>()
        };
        // Multiple filtered package, check if file/index filtering is applied
        if file_filter.is_some() || index_filter.is_some() || doc_index_filter.is_some() {
            bail!(
                "Cannot filter by file or index when multiple packages are specified. Matched packages: {:?}",
                package_names()
            );
        }
        if patch_file.is_some() {
            bail!(
                "Cannot apply patch file when multiple packages are specified. Matched packages: {:?}",
                package_names()
            );
        }
        for &pkg_id in &filtered_package_ids {
            out_filter.add_autodetermine_target(pkg_id, None, None);
        }
    } else {
        // No package matched
        warn!("no package found matching the given filters");
    }

    Ok(input_directive)
}

/// Calculate the user intent for the build system to construct.
///
/// Applies the package filter for the given package, file and index combination,
/// sets the package filter, and returns the list of build plan nodes.
///
/// This function couples intent calculation and filter generation, because the
/// both the test filter and user intent wants the same `BuildTarget` list, but
/// the earliest time we can get them is during intent calculation. Since the
/// fuzzy matching process is quite complex, we would avoid doing it twice.
fn calc_user_intent(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    main_modules: &[moonutil::mooncakes::ModuleId],
    cmd: &TestLikeSubcommand<'_>,
    out_filter: &mut TestFilter,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let &[main_module_id] = main_modules else {
        panic!("No multiple main modules are supported");
    };

    let packages = resolve_output
        .pkg_dirs
        .packages_for_module(main_module_id)
        .ok_or_else(|| anyhow::anyhow!("Cannot find the local module!"))?;
    let affected_packages = packages.values().copied();

    let directive = if let Some(file_filter) = cmd.explicit_file_filter {
        let test_index = cmd
            .index
            .map(TestIndex::Regular)
            .or(cmd.doc_index.map(TestIndex::DocTest));
        apply_explicit_file_filter(resolve_output, out_filter, file_filter, test_index)?;
        Default::default()
    } else if let Some(package_filter) = cmd.package {
        apply_list_of_filters(
            affected_packages,
            resolve_output,
            package_filter,
            cmd.file.as_deref(),
            *cmd.index,
            *cmd.doc_index,
            cmd.patch_file.as_deref(),
            out_filter,
        )?
    } else {
        // No filter: emit one intent per package (Test/Bench)
        let intents: Vec<_> = affected_packages.map(UserIntent::Test).collect();
        return Ok(intents.into());
    };

    // Generate intents for the filtered packages
    let intents = if let Some(filt) = out_filter.filter.as_ref() {
        use std::collections::HashSet;
        let mut pkgs = HashSet::new();
        for (target, _) in &filt.0 {
            pkgs.insert(target.package);
        }
        pkgs.into_iter().map(UserIntent::Test).collect::<Vec<_>>()
    } else {
        vec![]
    };
    Ok((intents, directive).into())
}

#[instrument(skip_all)]
pub(crate) fn run_test_or_bench_internal_legacy(
    cli: &UniversalFlags,
    cmd: TestLikeSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    display_backend_hint: Option<()>,
) -> anyhow::Result<i32> {
    // Run moon install before build
    let (resolved_env, dir_sync_result) = auto_sync(
        source_dir,
        cmd.auto_sync_flags,
        &RegistryConfig::load(),
        cli.quiet,
    )?;

    let run_mode = cmd.run_mode;

    let mut build_flags = cmd.build_flags.clone();
    if run_mode == RunMode::Bench && !build_flags.debug && !build_flags.release {
        build_flags.release = true;
    }

    // MAINTAINERS: Yes, this piece of code might result in both debug=true and
    // release=true. This is expected behavior and is required to make the
    // feature work, because of the current status of legacy flags setting.
    //
    // This piece will be thrown away very soon. If it's still here by 2026,
    // please do a full refactor.
    let compiler_flags = BuildFlags {
        debug: true,
        ..build_flags.clone()
    };
    let mut moonc_opt = super::get_compiler_flags(source_dir, &compiler_flags)?;
    moonc_opt.build_opt.debug_flag = !build_flags.release;
    moonc_opt.build_opt.enable_value_tracing = build_flags.enable_value_tracing;
    moonc_opt.build_opt.strip_flag = if build_flags.strip {
        true
    } else if build_flags.no_strip {
        false
    } else {
        build_flags.release
    };
    moonc_opt.link_opt.debug_flag = !build_flags.release;

    // TODO: remove this once LLVM backend is well supported
    if moonc_opt.build_opt.target_backend == TargetBackend::LLVM {
        eprintln!(
            "{}: LLVM backend is experimental and only supported on bleeding moonbit toolchain for now",
            "Warning".yellow()
        );
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

    // semantics: if single file filtering -- get package from file path, and
    // then filename from the, well, filename.
    let (filter_package, filter_file) = if let Some(file) = cmd.explicit_file_filter {
        let filename = file.file_name().map(|x| x.to_string_lossy().into_owned());
        // Note: We can't filter packages here because we don't have the full
        // list of packages to filter from. This has to be done after we have
        // scanned stuff.
        (None, filename)
    } else {
        let filter_package = cmd.package.clone().map(|it| it.into_iter().collect());
        let filter_file = cmd.file.clone();
        (filter_package, filter_file)
    };
    let filter_index = *cmd.index;
    let filter_doc_index = *cmd.doc_index;

    let test_opt = if run_mode == RunMode::Bench {
        Some(TestOpt {
            filter_package: filter_package.clone(),
            filter_file: filter_file.map(|x| x.to_owned()),
            filter_index,
            filter_doc_index,
            limit,
            test_failure_json: false,
            display_backend_hint,
            patch_file: None,
        })
    } else {
        Some(TestOpt {
            filter_package: filter_package.clone(),
            filter_file: filter_file.map(|x| x.to_owned()),
            filter_index,
            filter_doc_index,
            limit,
            test_failure_json: cmd.test_failure_json,
            display_backend_hint,
            patch_file: patch_file.clone(),
        })
    };
    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.to_path_buf(),
        raw_target_dir,
        target_dir: target_dir.clone(),
        test_opt,
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
        use_tcc_run: false,
        dynamic_stub_libs: None,
        render_no_loc: cmd.build_flags.render_no_loc,
    };

    let mut module = scan_with_x_build(
        false,
        &moonc_opt,
        &moonbuild_opt,
        &resolved_env,
        &dir_sync_result,
        &PrePostBuild::PreBuild,
    )?;

    let (package_filter, moonbuild_opt) = if let Some(file) = cmd.explicit_file_filter {
        let file = dunce::canonicalize(file)
            .context("failed to canonicalize the path specified by single file filter")?;
        // First, determine if it's a valid file path
        if !file.exists() {
            anyhow::bail!("File for filtering `{}` does not exist", file.display());
        }
        // Match the file to a package
        let (dir, filename) = if file.is_dir() {
            (file.as_path(), None)
        } else {
            (
                file.parent().expect("file must have a parent"),
                file.file_name(),
            )
        };

        let pkg = module.get_package_by_path(dir);
        if pkg.is_none() {
            anyhow::bail!(
                "Cannot find package for file `{}`, is it inside a package?",
                file.display()
            );
        }

        let pkg = pkg.unwrap();
        let filename = filename.map(|x| x.to_string_lossy().to_string());

        if let Some(filename) = filename.as_ref()
            && !pkg.files.contains_key(&file)
            && !pkg.test_files.contains_key(&file)
            && !pkg.wbtest_files.contains_key(&file)
            && !pkg.mbt_md_files.contains_key(&file)
        {
            eprintln!(
                "{}: cannot find file `{}` as a source file in package `{}`",
                "Warning".yellow(),
                filename,
                pkg.full_name()
            );
        }

        // Force package filter to the package containing the file, keep file filter as basename.
        let pkg_full_name = pkg.full_name().to_string();

        let moonbuild_opt = MoonbuildOpt {
            test_opt: Some(TestOpt {
                // override/force the package filter to the detected package
                filter_package: Some(HashSet::from([pkg_full_name.clone()])),
                filter_file: filename,
                // preserve the existing file/index/doc-index and other flags from earlier
                ..moonbuild_opt.test_opt.unwrap()
            }),
            ..moonbuild_opt
        };

        let package_filter: Option<Box<dyn for<'a> Fn(&'a _) -> _>> =
            Some(Box::new(move |p: &Package| p.full_name() == pkg_full_name));
        (package_filter, moonbuild_opt)
    } else if let Some(filter_package) = moonbuild_opt
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
                        .map(|p| format!("`{p}`"))
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

        let package_filter: Option<Box<dyn for<'a> Fn(&'a _) -> _>> =
            Some(Box::new(move |pkg: &Package| {
                final_set.contains(&pkg.full_name())
            }));
        (package_filter, moonbuild_opt)
    } else {
        (None, moonbuild_opt)
    };

    let mut use_tcc_run = moonc_opt.build_opt.debug_flag
        && moonbuild_opt.run_mode == RunMode::Test
        && moonc_opt.build_opt.target_backend == TargetBackend::Native;

    for (_, pkg) in module.get_filtered_packages_mut(package_filter) {
        // do a pre-check to ensure that enabling fast cc mode (using tcc for debug testing)
        // will not break the user's expectation on their control over
        // c compilers and flags
        let existing_native = pkg.link.as_ref().and_then(|link| link.native.as_ref());
        if let Some(n) = existing_native {
            let old_flag = use_tcc_run;
            use_tcc_run &= n.cc.is_none() && n.cc_flags.is_none() && n.cc_link_flags.is_none();
            if old_flag != use_tcc_run {
                eprintln!(
                    "{}: package `{}` has native cc, cc-flags, or cc-link-flags. `tcc run` will be disabled",
                    "Warning".yellow(),
                    pkg.full_name()
                );
            }
        }

        if pkg.is_third_party {
            continue;
        }

        if cmd.build_flags.enable_value_tracing
            && let Some(filter_package) = moonbuild_opt
                .test_opt
                .as_ref()
                .and_then(|it| it.filter_package.as_ref())
            && filter_package.contains(&pkg.full_name())
        {
            pkg.enable_value_tracing = true;
        }

        pkg.patch_file = patch_file.clone();

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

    let all_stubs_dyn_deps = moonutil::common::set_native_backend_link_flags(
        run_mode,
        moonc_opt.build_opt.target_backend,
        &mut module,
    )?;

    let moonbuild_opt = MoonbuildOpt {
        use_tcc_run,
        dynamic_stub_libs: Some(all_stubs_dyn_deps),
        ..moonbuild_opt
    };

    // add coverage libs if needed
    moonbuild::r#gen::gen_runtest::add_coverage_to_core_if_needed(&mut module, &moonc_opt)?;

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
        cli.quiet,
    );

    if cli.trace {
        trace::close();
    }

    res
}

#[instrument(level = Level::DEBUG, skip_all)]
fn do_run_test(
    moonc_opt: MooncOpt,
    moonbuild_opt: MoonbuildOpt,
    build_only: bool,
    auto_update: bool,
    module: ModuleDB,
    verbose: bool,
    quiet: bool,
) -> anyhow::Result<i32> {
    let backend_hint = moonbuild_opt
        .test_opt
        .as_ref()
        .and_then(|opt| opt.display_backend_hint.as_ref())
        .map(|_| moonc_opt.build_opt.target_backend.to_backend_ext());

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

    print_test_summary(total, passed, quiet, backend_hint);

    if passed == total {
        Ok(0)
    } else {
        // don't bail! here, use no-zero exit code to indicate test failed
        Ok(2)
    }
}
