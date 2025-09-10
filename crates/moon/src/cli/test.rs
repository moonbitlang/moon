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
use indexmap::IndexMap;
use moonbuild::dry_run;
use moonbuild::entry;
use moonbuild_rupes_recta::model::BuildPlanNode;
use moonbuild_rupes_recta::model::PackageId;
use moonbuild_rupes_recta::model::TargetKind;
use mooncake::pkg::sync::auto_sync;
use mooncake::pkg::sync::auto_sync_for_single_mbt_md;
use moonutil::common::PrePostBuild;
use moonutil::common::{
    lower_surface_targets, parse_front_matter_config, FileLock, GeneratedTestDriver, MbtMdHeader,
    MoonbuildOpt, MooncOpt, OutputFormat, RunMode, TargetBackend, TestOpt, MOONBITLANG_CORE,
};
use moonutil::common::{BLACKBOX_TEST_DRIVER, DOT_MBT_DOT_MD, SINGLE_FILE_TEST_PACKAGE};
use moonutil::cond_expr::CompileCondition;
use moonutil::cond_expr::OptLevel;
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::module::ModuleDB;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use moonutil::package::Package;
use moonutil::path::PathComponent;
use n2::trace;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::cli::pre_build::scan_with_x_build;
use crate::rr_build;
use crate::rr_build::preconfig_compile;
use crate::rr_build::BuildConfig;
use crate::run::TestFilter;
use crate::run::TestIndex;

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
    #[clap(short, long, num_args(0..))]
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

    /// Run test in single file (.mbt or .mbt.md)
    pub single_file: Option<PathBuf>,
}

pub fn run_test(cli: UniversalFlags, cmd: TestSubcommand) -> anyhow::Result<i32> {
    let (source_dir, target_dir) = if let Some(ref single_file_path) = cmd.single_file {
        let single_file_path = &dunce::canonicalize(single_file_path).context(format!(
            "failed to run tests for a nonexistent file '{}'",
            single_file_path.display()
        ))?;
        let source_dir = single_file_path
            .parent()
            .context(format!(
                "impossible to run tests for a non-file '{}'",
                single_file_path.display(),
            ))?
            .to_path_buf();
        let target_dir = source_dir.join("target");
        (source_dir, target_dir)
    } else {
        let dir = cli.source_tgt_dir.try_into_package_dirs()?;
        (dir.source_dir, dir.target_dir)
    };

    if cmd.doc_test {
        eprintln!(
            "{}: --doc flag is deprecated and will be removed in the future, please use `moon test` directly",
            "Warning".yellow(),
        );
    }

    let Some(surface_targets) = &cmd.build_flags.target else {
        return run_test_internal(&cli, &cmd, &source_dir, &target_dir, None);
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
        let x = run_test_internal(&cli, &cmd, &source_dir, &target_dir, display_backend_hint)
            .context(format!("failed to run test for target {t:?}"))?;
        ret_value = ret_value.max(x);
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
    if cmd.single_file.is_some() {
        run_test_in_single_file(cli, cmd)
    } else {
        run_test_or_bench_internal(
            cli,
            cmd.into(),
            source_dir,
            target_dir,
            display_backend_hint,
        )
    }
}

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

pub(crate) fn run_test_or_bench_internal(
    cli: &UniversalFlags,
    cmd: TestLikeSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    display_backend_hint: Option<()>,
) -> anyhow::Result<i32> {
    // move the conflict detection logic here since we want specific `index` only for single file test
    if cmd.package.is_none() && cmd.file.is_some() {
        anyhow::bail!("`--file` must be used with `--package`");
    }
    if cmd.file.is_none() && cmd.index.is_some() {
        anyhow::bail!("`--index` must be used with `--file`");
    }

    if cli.unstable_feature.rupes_recta {
        run_test_rr(cli, &cmd, source_dir, target_dir, display_backend_hint)
    } else {
        run_test_or_bench_internal_legacy(cli, cmd, source_dir, target_dir, display_backend_hint)
    }
}

fn run_test_rr(
    cli: &UniversalFlags,
    cmd: &TestLikeSubcommand<'_>,
    source_dir: &Path,
    target_dir: &Path,
    display_backend_hint: Option<()>, // FIXME: unsure why it's option but as-is for now
) -> Result<i32, anyhow::Error> {
    let preconfig = preconfig_compile(
        cmd.auto_sync_flags,
        cli,
        cmd.build_flags,
        target_dir,
        OptLevel::Debug,
        RunMode::Test,
    );

    validate_filtering(
        cmd.package.as_deref(),
        cmd.file.as_ref(),
        *cmd.index,
        *cmd.doc_index,
    )?;

    let filter = cmd.package.clone();
    let (build_meta, build_graph) = rr_build::plan_build(
        preconfig,
        &cli.unstable_feature,
        source_dir,
        target_dir,
        Box::new(move |resolved, main_modules| calc_user_intent(resolved, main_modules, filter)),
    )?;

    if cli.dry_run {
        rr_build::print_dry_run(&build_graph, &build_meta.artifacts, source_dir, target_dir);
        // The legacy behavior does not print the test commands, so we skip it too.

        Ok(0)
    } else {
        let result = rr_build::execute_build(
            &BuildConfig::from_flags(cmd.build_flags),
            build_graph,
            target_dir,
        )?;

        if !result.successful() || cmd.build_only {
            return Ok(result.return_code_for_success());
        }

        let filter = TestFilter {
            file: cmd.file.clone(),
            index: (*cmd.index)
                .map(TestIndex::Regular)
                .or_else(|| (*cmd.doc_index).map(TestIndex::DocTest)),
        };

        // Run tests using artifacts
        let test_result = crate::run::run_tests(&build_meta, target_dir, &filter, cli.verbose)?;

        let backend_hint = display_backend_hint
            .and(cmd.build_flags.target_backend)
            .map(|t| t.to_backend_ext());

        print_test_summary(
            test_result.total,
            test_result.passed,
            cli.quiet,
            backend_hint,
        );

        if test_result.passed() {
            Ok(0)
        } else {
            Ok(1)
        }
    }
}

fn validate_filtering(
    package: Option<&[String]>,
    file: Option<&String>,
    index: Option<u32>,
    doc_index: Option<u32>,
) -> anyhow::Result<()> {
    if package.is_none() && (file.is_some() || index.is_some() || doc_index.is_some()) {
        anyhow::bail!("Cannot filter by file or index without specifying a package");
    }
    if let Some(pkgs) = package {
        if pkgs.len() != 1 && (file.is_some() || index.is_some() || doc_index.is_some()) {
            anyhow::bail!("Cannot filter by file or index when multiple packages are specified");
        }
    }
    if file.is_none() && (index.is_some() || doc_index.is_some()) {
        anyhow::bail!("Cannot filter by index without specifying a file");
    }
    if index.is_some() && doc_index.is_some() {
        anyhow::bail!("Cannot filter by both regular index and doc test index");
    }
    Ok(())
}

fn calc_user_intent(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    main_modules: &[moonutil::mooncakes::ModuleId],
    package_filter: Option<Vec<String>>,
) -> Result<Vec<BuildPlanNode>, anyhow::Error> {
    let &[main_module_id] = main_modules else {
        panic!("No multiple main modules are supported");
    };

    let packages = resolve_output
        .pkg_dirs
        .packages_for_module(main_module_id)
        .ok_or_else(|| anyhow::anyhow!("Cannot find the local module!"))?;

    // Fuzzy matching --
    // for package names to determine the final set of packages to test. This
    // should not be a perf bottleneck; use a more generic way to handle.
    let nodes: Box<dyn Iterator<Item = PackageId>> = if let Some(filter) = package_filter {
        // We deliberately didn't allow a string to be parsed into a package
        // name, because different package may have a same name. However, in a
        // module the package name should be unique, so we can make a map here.
        let name_map = packages
            .values()
            .map(|id| {
                let name = resolve_output.pkg_dirs.get_package(*id).fqn.to_string();
                (name, *id)
            })
            .collect::<HashMap<_, _>>();
        let mut res = vec![];

        for s in filter {
            if let Some(&id) = name_map.get(&s) {
                // exact matching
                res.push(id);
            } else {
                let all_names = name_map.keys().map(|k| k.as_str());
                let xs = moonutil::fuzzy_match::fuzzy_match(s.as_str(), all_names);
                if let Some(xs) = xs {
                    for name in xs {
                        if let Some(&id) = name_map.get(&name) {
                            res.push(id);
                        } else {
                            unreachable!("All names being fuzzy matched should be contained in the name list")
                        }
                    }
                } else {
                    log::warn!("no package found matching test filter `{}`", s);
                }
            }
        }

        Box::new(res.into_iter())
    } else {
        Box::new(packages.values().copied())
    };

    let nodes = nodes
        .flat_map(|pkg_id| {
            [
                TargetKind::InlineTest,
                TargetKind::WhiteboxTest,
                TargetKind::BlackboxTest,
            ]
            .into_iter()
            .flat_map(move |x| {
                [
                    BuildPlanNode::make_executable(pkg_id.build_target(x)),
                    BuildPlanNode::generate_test_info(pkg_id.build_target(x)),
                ]
            })
        })
        .collect();
    Ok(nodes)
}

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

    let build_flags = BuildFlags {
        debug: true,
        ..cmd.build_flags.clone()
    };
    let mut moonc_opt = super::get_compiler_flags(source_dir, &build_flags)?;
    // release is 'false' by default, so we will run test at debug mode(to gain more detailed stack trace info), unless `--release` is specified
    // however, other command like build, check, run, etc, will run at release mode by default
    moonc_opt.build_opt.debug_flag = !cmd.build_flags.release;
    moonc_opt.build_opt.enable_value_tracing = cmd.build_flags.enable_value_tracing;
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
        eprintln!("{}: LLVM backend is experimental and only supported on bleeding moonbit toolchain for now", "Warning".yellow());
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
    let filter_file = cmd.file;
    let filter_index = *cmd.index;
    let filter_doc_index = *cmd.doc_index;

    let test_opt = if run_mode == RunMode::Bench {
        Some(TestOpt {
            filter_package: filter_package.clone(),
            filter_file: filter_file.clone(),
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
            filter_file: filter_file.clone(),
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

        let package_filter = Some(move |pkg: &Package| final_set.contains(&pkg.full_name()));
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

        if cmd.build_flags.enable_value_tracing {
            if let Some(filter_package) = moonbuild_opt
                .test_opt
                .as_ref()
                .and_then(|it| it.filter_package.as_ref())
            {
                if filter_package.contains(&pkg.full_name()) {
                    pkg.enable_value_tracing = true;
                }
            }
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
        cli.quiet,
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
