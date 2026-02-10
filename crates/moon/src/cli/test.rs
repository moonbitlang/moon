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

use crate::filter::canonicalize_with_filename;
use crate::filter::filter_pkg_by_dir;
use crate::filter::match_packages_with_fuzzy;
use crate::rr_build;
use crate::rr_build::preconfig_compile;
use crate::rr_build::{BuildConfig, CalcUserIntentOutput};
use crate::run::TestFilter;
use crate::run::TestIndex;
use crate::run::TestOutlineEntry;
use crate::run::collect_test_outline;
use crate::run::perform_promotion;
use anyhow::Context;
use anyhow::bail;
use clap::builder::ArgPredicate;
use colored::Colorize;
use moonbuild_rupes_recta::build_plan::InputDirective;
use moonbuild_rupes_recta::intent::UserIntent;
use moonbuild_rupes_recta::model::BuildPlanNode;
use moonbuild_rupes_recta::model::BuildTarget;
use moonbuild_rupes_recta::model::PackageId;
use moonutil::common::BUILD_DIR;
use moonutil::common::{
    FileLock, RunMode, TargetBackend, TestArtifacts, TestIndexRange, lower_surface_targets,
};
use moonutil::mooncakes::RegistryConfig;
use moonutil::mooncakes::sync::AutoSyncFlags;
use std::path::{Path, PathBuf};
use tracing::{Level, debug, info, instrument, trace, warn};

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

fn print_test_outline(entries: &[TestOutlineEntry]) {
    if entries.is_empty() {
        eprintln!("{}: no test entry found.", "Warning".yellow().bold());
        return;
    }

    for (i, entry) in entries.iter().enumerate() {
        let line = entry
            .line_number
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?".to_string());
        let mut line_out = format!(
            "{:>4}. {} {}:{} index={}",
            i + 1,
            entry.package,
            entry.file,
            line,
            entry.index
        );
        if let Some(name) = &entry.name {
            line_out.push_str(&format!(" name={name:?}"));
        }
        println!("{line_out}");
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

    /// Run only the index-th test in the file. Accepts a single index or a left-inclusive
    /// right-exclusive range like `0-2`. Only valid when `--file` is also specified.
    /// Implies `--include-skipped`.
    #[clap(short, long)]
    pub index: Option<TestIndexRange>,

    /// Run only the index-th doc test in the file. Only valid when `--file` is also specified.
    /// Implies `--include-skipped`.
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

    /// Print the outline of tests to be executed and exit
    #[clap(long, conflicts_with_all = ["build_only", "update", "test_failure_json"])]
    pub outline: bool,

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

    /// Include skipped tests. Automatically implied when `--[doc-]index` is set.
    #[clap(long)]
    #[clap(default_value_if("index", ArgPredicate::IsPresent, "true"))]
    #[clap(default_value_if("doc_index", ArgPredicate::IsPresent, "true"))]
    pub include_skipped: bool,

    /// Run only tests whose name matches the given glob pattern.
    /// Supports '*' (matches any sequence) and '?' (matches any single character).
    #[clap(short = 'F', long)]
    pub filter: Option<String>,
}

#[instrument(skip_all)]
pub fn run_test(cli: UniversalFlags, cmd: TestSubcommand) -> anyhow::Result<i32> {
    let result = run_test_impl(&cli, &cmd);
    if crate::run::shutdown_requested() {
        return Ok(130);
    }
    result
}

#[instrument(skip_all)]
fn run_test_impl(cli: &UniversalFlags, cmd: &TestSubcommand) -> anyhow::Result<i32> {
    info!(
        update = cmd.update,
        build_only = cmd.build_only,
        doc_test = cmd.doc_test,
        package_filters = cmd.package.as_ref().map(|p| p.len()).unwrap_or(0),
        has_single_file = cmd.single_file.is_some(),
        "starting moon test command"
    );
    // Check if we're running within a project
    let dirs = match cli.source_tgt_dir.try_into_package_dirs() {
        Ok(dirs) => dirs,
        Err(e @ moonutil::dirs::PackageDirsError::NotInProject(_)) => {
            // Now we're talking about real single-file scenario.
            if cmd.single_file.is_some() {
                info!("delegating to single-file test runner");
                return run_test_in_single_file(cli, cmd);
            } else {
                return Err(e.into());
            }
        }
        Err(e) => {
            return Err(e.into());
        }
    };

    debug!(
        source = %dirs.source_dir.display(),
        target = %dirs.target_dir.display(),
        "resolved package directories"
    );

    if cmd.doc_test {
        eprintln!(
            "{}: --doc flag is deprecated and will be removed in the future, please use `moon test` directly",
            "Warning".yellow(),
        );
    }

    if cmd.build_flags.target.is_empty() {
        debug!("no explicit backend target provided; using defaults");
        return run_test_internal(cli, cmd, &dirs.source_dir, &dirs.target_dir, None, None);
    }
    let surface_targets = &cmd.build_flags.target;
    let targets = lower_surface_targets(surface_targets);
    if cmd.update && targets.len() > 1 {
        return Err(anyhow::anyhow!("cannot update test on multiple targets"));
    }
    let display_backend_hint = if targets.len() > 1 { Some(()) } else { None };

    let mut ret_value = 0;
    for t in targets {
        info!(backend = ?t, "running tests for backend");
        let x = run_test_internal(
            cli,
            cmd,
            &dirs.source_dir,
            &dirs.target_dir,
            display_backend_hint,
            Some(t),
        )
        .context(format!("failed to run test for target {t:?}"))?;
        ret_value = ret_value.max(x);
    }
    debug!(exit_code = ret_value, "completed moon test command");
    Ok(ret_value)
}

#[instrument(skip_all)]
fn run_test_internal(
    cli: &UniversalFlags,
    cmd: &TestSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    display_backend_hint: Option<()>,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    debug!(
        backend = ?selected_target_backend,
        build_only = cmd.build_only,
        "entering run_test_internal"
    );
    let exit_code = run_test_or_bench_internal(
        cli,
        cmd.into(),
        source_dir,
        target_dir,
        display_backend_hint,
        selected_target_backend,
    )?;
    trace!(exit_code, "run_test_internal finished");
    Ok(exit_code)
}

#[instrument(level = Level::DEBUG, skip_all)]
fn run_test_in_single_file(cli: &UniversalFlags, cmd: &TestSubcommand) -> anyhow::Result<i32> {
    if cmd.outline && cli.dry_run {
        anyhow::bail!("`--outline` cannot be used with `--dry-run`");
    }
    if cmd.outline && !cli.unstable_feature.rupes_recta {
        anyhow::bail!("`--outline` is only supported with Rupes Recta (-Z rupes_recta)");
    }
    run_test_in_single_file_rr(cli, cmd)
}

#[instrument(level = Level::DEBUG, skip_all)]
fn run_test_in_single_file_rr(cli: &UniversalFlags, cmd: &TestSubcommand) -> anyhow::Result<i32> {
    let path = cmd
        .single_file
        .as_ref()
        .expect("single_file should be set in single-file mode");
    let single_file_path = dunce::canonicalize(path)
        .with_context(|| format!("failed to resolve file path `{}`", path.display()))?;
    let source_dir = single_file_path
        .parent()
        .context("file path must have a parent directory")?
        .to_path_buf();
    let raw_target_dir = source_dir.join(BUILD_DIR);
    std::fs::create_dir_all(&raw_target_dir)
        .context("failed to create target directory for single-file test")?;

    let mut filter = TestFilter {
        name_filter: cmd.filter.clone(),
        ..Default::default()
    };

    // Resolve synthesized single-file project
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        RegistryConfig::load(),
        false,
        cmd.build_flags.enable_coverage,
    );
    let (resolved, backend) = moonbuild_rupes_recta::resolve::resolve_single_file_project(
        &resolve_cfg,
        &single_file_path,
        false,
    )?;
    let selected_target_backend = cmd.build_flags.resolve_single_target_backend()?.or(backend);

    let mut preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        &raw_target_dir,
        RunMode::Test,
    );
    // Enable tcc-run to match legacy debug test graph shape
    preconfig.try_tcc_run = true;

    // Plan build: single UserIntent::Test for synthesized package; apply file/index filters
    let (build_meta, build_graph) = rr_build::plan_build_from_resolved(
        preconfig,
        &cli.unstable_feature,
        &raw_target_dir,
        Box::new(|r, _tb| {
            let m_packages = r
                .pkg_dirs
                .packages_for_module(r.local_modules()[0])
                .expect("Local module must exist");
            let pkg = *m_packages
                .iter()
                .next()
                .expect("Single-file project must synthesize exactly one package")
                .1;

            let test_index = if let Some(index) = cmd.index {
                Some(TestIndex::Regular(index))
            } else if let Some(id) = cmd.doc_index {
                Some(TestIndex::DocTest(TestIndexRange::from_single(id)?))
            } else {
                None
            };
            let filename = single_file_path
                .file_name()
                .expect("single file path should have a filename")
                .to_string_lossy();
            filter.add_autodetermine_target(pkg, Some(&filename), test_index);

            let trace_pkg = if cmd.build_flags.enable_value_tracing {
                Some(pkg)
            } else {
                None
            };
            let directive =
                rr_build::build_patch_directive_for_package(pkg, false, trace_pkg, None, true)?;

            Ok((vec![UserIntent::Test(pkg)], directive).into())
        }),
        resolved,
    )?;

    let test_cmd: TestLikeSubcommand<'_> = cmd.into();
    rr_test_from_plan(
        cli,
        &test_cmd,
        &source_dir,
        &raw_target_dir,
        None,
        &build_meta,
        build_graph,
        filter,
    )
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
    pub index: &'a Option<TestIndexRange>,
    pub doc_index: &'a Option<u32>,
    pub update: bool,
    pub limit: u32,
    pub auto_sync_flags: &'a AutoSyncFlags,
    pub build_only: bool,
    pub no_parallelize: bool,
    pub outline: bool,
    pub test_failure_json: bool,
    pub patch_file: &'a Option<PathBuf>,
    pub include_skipped: bool,
    /// Glob pattern to filter tests by name
    pub filter: &'a Option<String>,
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
            outline: cmd.outline,
            test_failure_json: cmd.test_failure_json,
            patch_file: &cmd.patch_file,
            include_skipped: cmd.include_skipped,
            filter: &cmd.filter,
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
            outline: false,
            test_failure_json: false,
            patch_file: &None,
            include_skipped: false,
            filter: &None,
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
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    let explicit_file = cmd.explicit_file_filter.map(|p| p.display().to_string());
    debug!(
        run_mode = ?cmd.run_mode,
        update = cmd.update,
        build_only = cmd.build_only,
        package_filters = cmd.package.as_ref().map(|p| p.len()).unwrap_or(0),
        explicit_file = explicit_file.as_deref(),
        "entering run_test_or_bench_internal"
    );
    trace!(
        index = ?cmd.index,
        doc_index = cmd.doc_index,
        no_parallelize = cmd.no_parallelize,
        "cli filter state"
    );
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
    if cmd.outline && cli.dry_run {
        anyhow::bail!("`--outline` cannot be used with `--dry-run`");
    }
    if cmd.outline && !cli.unstable_feature.rupes_recta {
        anyhow::bail!("`--outline` is only supported with Rupes Recta (-Z rupes_recta)");
    }

    debug!(
        rupes_recta = cli.unstable_feature.rupes_recta,
        "selecting test runner implementation"
    );
    run_test_rr(
        cli,
        &cmd,
        source_dir,
        target_dir,
        display_backend_hint,
        selected_target_backend,
    )
}

#[instrument(skip_all)]
fn run_test_rr(
    cli: &UniversalFlags,
    cmd: &TestLikeSubcommand<'_>,
    source_dir: &Path,
    target_dir: &Path,
    display_backend_hint: Option<()>, // FIXME: unsure why it's option but as-is for now
    selected_target_backend: Option<TargetBackend>,
) -> Result<i32, anyhow::Error> {
    info!(run_mode = ?cmd.run_mode, update = cmd.update, build_only = cmd.build_only, "starting rupes-recta test run");
    let is_bench = cmd.run_mode == RunMode::Bench;

    // MAINTAINERS: This is to match the legacy behavior of `moon test` always
    // emitting debug info regardless of `--release` flag. This may result in
    // both `debug=true` and `release=true` and it's expected behavior. It
    // should be removed once https://github.com/moonbitlang/moon/pull/1153 is
    // in place.
    let build_flags = BuildFlags {
        no_strip: !cmd.build_flags.strip && !cmd.build_flags.release,
        ..cmd.build_flags.clone()
    };

    let mut preconfig = preconfig_compile(
        cmd.auto_sync_flags,
        cli,
        &build_flags,
        selected_target_backend,
        target_dir,
        if cmd.run_mode == RunMode::Bench {
            RunMode::Bench
        } else {
            RunMode::Test
        },
    );
    // Enable tcc-run for tests regardless of dry-run so the graph shape matches legacy.
    if !is_bench {
        preconfig.try_tcc_run = true;
    }

    let mut filter = TestFilter {
        name_filter: cmd.filter.clone(),
        ..Default::default()
    };
    let (build_meta, build_graph) = rr_build::plan_build(
        preconfig,
        &cli.unstable_feature,
        source_dir,
        target_dir,
        Box::new(|resolved, target_backend| {
            calc_user_intent(resolved, cmd, &mut filter, target_backend)
        }),
    )?;
    debug!(
        artifact_count = build_meta.artifacts.len(),
        "planned rupes-recta build graph"
    );

    rr_test_from_plan(
        cli,
        cmd,
        source_dir,
        target_dir,
        display_backend_hint,
        &build_meta,
        build_graph,
        filter,
    )
}

/// The nodes wanted to run a test for a build target
fn node_from_target(x: BuildTarget) -> [BuildPlanNode; 2] {
    [
        BuildPlanNode::make_executable(x),
        BuildPlanNode::generate_test_info(x),
    ]
}

/// Apply explicit PATH filter (acts as package and optional file filter).
/// `test_index` selects test indices (regular/doc) when PATH is a file.
#[instrument(level = "debug", skip(resolve_output, out_filter))]
fn apply_explicit_file_filter(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    out_filter: &mut TestFilter,
    file_filter: &Path,
    test_index: Option<TestIndex>,
) -> Result<(), anyhow::Error> {
    let (dir, filename) = canonicalize_with_filename(file_filter)?;
    debug!(dir = %dir.display(), filename = ?filename, "resolved explicit file filter path");

    let pkg = filter_pkg_by_dir(resolve_output, &dir)?;
    debug!(package = ?pkg, file = filename.as_deref(), "resolved explicit filter target");

    out_filter.add_autodetermine_target(pkg, filename.as_deref(), test_index);
    Ok(())
}

/// Apply the hierarchy of filters of packages, file and index
#[allow(clippy::too_many_arguments)]
#[instrument(level = "debug", skip(affected_packages, resolve_output, out_filter))]
fn apply_list_of_filters(
    affected_packages: &[PackageId],
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    package_filter: &[String],
    file_filter: Option<&str>,
    index_filter: Option<TestIndexRange>,
    doc_index_filter: Option<u32>,
    patch_file: Option<&Path>,
    value_tracing: bool,
    out_filter: &mut TestFilter,
) -> Result<InputDirective, anyhow::Error> {
    let package_matches = match_packages_with_fuzzy(
        resolve_output,
        affected_packages.iter().copied(),
        package_filter,
    );
    let filtered_package_ids = package_matches.matched;
    trace!(
        filtered_packages = filtered_package_ids.len(),
        "package filters resolved"
    );

    // Calculate resulting filter & target list
    let mut input_directive = InputDirective::default();
    #[allow(clippy::comparison_chain)]
    if filtered_package_ids.len() == 1 {
        // Single filtered package, can apply file/index filtering
        let pkg_id = filtered_package_ids[0];
        if let Some(range) = index_filter {
            out_filter.add_autodetermine_target(
                pkg_id,
                file_filter,
                Some(TestIndex::Regular(range)),
            );
        } else if let Some(id) = doc_index_filter {
            let range = TestIndexRange::from_single(id)?;
            out_filter.add_autodetermine_target(
                pkg_id,
                file_filter,
                Some(TestIndex::DocTest(range)),
            );
        } else {
            out_filter.add_autodetermine_target(pkg_id, file_filter, None);
        }
        // Currently, value tracing is only supported for single package testing
        // It's not sure whether we should support it for multiple packages
        let trace_pkg = if value_tracing { Some(pkg_id) } else { None };
        input_directive =
            rr_build::build_patch_directive_for_package(pkg_id, false, trace_pkg, patch_file, true)
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
        warn!(
            "package `{}` not found, make sure you have spelled it correctly, e.g. `moonbitlang/core/hashmap`(exact match) or `hashmap`(fuzzy match)",
            package_filter.join(", ")
        );
    }
    trace!("finished building package directive");

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
#[instrument(level = "debug", skip(resolve_output, cmd, out_filter))]
fn calc_user_intent(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &TestLikeSubcommand<'_>,
    out_filter: &mut TestFilter,
    target_backend: moonutil::common::TargetBackend,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let &[main_module_id] = resolve_output.local_modules() else {
        panic!("No multiple main modules are supported");
    };

    let packages = resolve_output
        .pkg_dirs
        .packages_for_module(main_module_id)
        .ok_or_else(|| anyhow::anyhow!("Cannot find the local module!"))?;
    debug!(
        package_count = packages.len(),
        "calculating user intent for module"
    );
    let affected_packages: Vec<_> = packages.values().copied().collect();

    let directive = if let Some(file_filter) = cmd.explicit_file_filter {
        let test_index = if let Some(index) = cmd.index {
            Some(TestIndex::Regular(*index))
        } else if let Some(id) = cmd.doc_index {
            Some(TestIndex::DocTest(TestIndexRange::from_single(*id)?))
        } else {
            None
        };
        apply_explicit_file_filter(resolve_output, out_filter, file_filter, test_index)?;
        trace!("explicit file filter applied");
        Default::default()
    } else if let Some(package_filter) = cmd.package {
        let value_tracing = cmd.build_flags.enable_value_tracing;
        apply_list_of_filters(
            &affected_packages,
            resolve_output,
            package_filter.as_slice(),
            cmd.file.as_deref(),
            *cmd.index,
            *cmd.doc_index,
            cmd.patch_file.as_deref(),
            value_tracing,
            out_filter,
        )?
    } else {
        // No filter: emit one intent per package (Test/Bench)
        let intents: Vec<_> = affected_packages
            .iter()
            .copied()
            .map(UserIntent::Test)
            .collect();
        debug!(intent_count = intents.len(), "generated default intents");
        return Ok(intents.into());
    };

    // Generate intents for the filtered packages
    let intents = if let Some(filt) = out_filter.filter.as_ref() {
        use std::collections::HashSet;
        let mut pkgs = HashSet::new();
        for (target, _) in &filt.0 {
            pkgs.insert(target.package);
        }
        trace!(
            package_count = pkgs.len(),
            "building intents from filtered targets"
        );
        pkgs.into_iter().map(UserIntent::Test).collect::<Vec<_>>()
    } else {
        vec![]
    };
    debug!(intent_count = intents.len(), "calculated user intent");
    Ok((intents, directive).into())
}

#[instrument(level = Level::DEBUG, skip_all)]
#[allow(clippy::too_many_arguments)] // FIXME
fn rr_test_from_plan(
    cli: &UniversalFlags,
    cmd: &TestLikeSubcommand<'_>,
    source_dir: &Path,
    target_dir: &Path,
    display_backend_hint: Option<()>,
    build_meta: &rr_build::BuildMeta,
    build_graph: rr_build::BuildInput,
    filter: TestFilter,
) -> Result<i32, anyhow::Error> {
    // Dry-run: share the same routine
    if cli.dry_run {
        rr_build::print_dry_run(
            &build_graph,
            build_meta.artifacts.values(),
            source_dir,
            target_dir,
        );
        // The legacy behavior does not print the test commands, so we skip it too.
        return Ok(0);
    }

    let _lock = FileLock::lock(target_dir)?;
    // Generate the all_pkgs.json for indirect dependency resolution
    // before executing the build
    rr_build::generate_all_pkgs_json(target_dir, build_meta, cmd.run_mode)?;

    let build_config = BuildConfig::from_flags(cmd.build_flags, &cli.unstable_feature, cli.verbose);

    // since n2 build consumes the graph, we back it up for reruns
    let build_graph_backup = cmd.update.then(|| build_graph.clone());
    let result = rr_build::execute_build(&build_config, build_graph, target_dir)?;
    debug!(
        success = result.successful(),
        exit_code = result.return_code_for_success(),
        "executed rupes-recta build"
    );

    if !result.successful() {
        return Ok(result.return_code_for_success());
    }

    if cmd.outline {
        let entries = collect_test_outline(
            build_meta,
            &filter,
            cmd.include_skipped,
            cmd.run_mode == RunMode::Bench,
        )?;
        print_test_outline(&entries);
        return Ok(0);
    }

    if cmd.build_only {
        // Match legacy behavior: create JS wrappers and print test artifacts as JSON
        let test_artifacts = collect_test_artifacts_for_build_only(build_meta, target_dir)?;
        println!("{}", serde_json_lenient::to_string(&test_artifacts)?);
        return Ok(0);
    }

    let mut test_result = crate::run::run_tests(
        build_meta,
        source_dir,
        target_dir,
        &filter,
        cmd.include_skipped,
        cmd.run_mode == RunMode::Bench,
        cli.verbose,
        cmd.no_parallelize,
        cmd.build_flags.jobs,
    )?;
    let _initial_summary = test_result.summary();

    let backend_hint = display_backend_hint
        .map(|_| TargetBackend::from(build_meta.target_backend).to_backend_ext());

    if cmd.update {
        let mut loop_count = 1; // matching legacy; we already have 1 test run before
        let mut last_test_result = None;
        loop {
            // Promote test results
            let promotion_source = last_test_result.as_ref().unwrap_or(&test_result);
            let (rerun_count, rerun_filter_raw) =
                perform_promotion(&build_meta.resolve_output.pkg_dirs, promotion_source)
                    .expect("Failed to promote tests");
            debug!(
                rerun_count,
                pending_targets = rerun_filter_raw.0.len(),
                "promotion pass completed"
            );
            if rerun_filter_raw.is_empty() {
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

            // Get the graph from backup
            let build_graph = build_graph_backup
                .as_ref()
                .cloned()
                .expect("build graph backup should be present when update is true");

            // Calculate which files to rebuild
            let want_files = rerun_filter_raw
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
                    trace!("requesting rerun artifacts");
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
                filter: Some(rerun_filter_raw),
                name_filter: cmd.filter.clone(),
            };
            let new_test_result = crate::run::run_tests(
                build_meta,
                source_dir,
                target_dir,
                &rerun_filter,
                cmd.include_skipped,
                cmd.run_mode == RunMode::Bench,
                cli.verbose,
                cmd.no_parallelize,
                cmd.build_flags.jobs,
            )?;
            let _rerun_summary = new_test_result.summary();

            // Merge test results
            test_result.merge(&new_test_result);
            last_test_result = Some(new_test_result);
        }
    }

    test_result.print_result(build_meta, cli.verbose, cmd.test_failure_json);
    let summary = test_result.summary();
    print_test_summary(summary.total, summary.passed, cli.quiet, backend_hint);

    if summary.total == summary.passed {
        Ok(0)
    } else {
        Ok(2)
    }
}

/// Collect test artifacts for --build-only mode, matching legacy behavior.
/// For JS backend, creates .cjs wrapper files and returns those paths.
/// For other backends, returns the executable paths directly.
/// Only includes artifacts that have actual tests (skips empty test executables).
fn collect_test_artifacts_for_build_only(
    build_meta: &rr_build::BuildMeta,
    target_dir: &Path,
) -> anyhow::Result<TestArtifacts> {
    use moonbuild_rupes_recta::model::RunBackend;
    use moonutil::common::MooncGenTestInfo;

    let mut artifacts_path = vec![];

    // Gather test executables (only MakeExecutable nodes)
    for (node, node_artifacts) in &build_meta.artifacts {
        if !matches!(node, BuildPlanNode::MakeExecutable(_)) {
            continue;
        }

        let target = node
            .extract_target()
            .expect("MakeExecutable should have a build target");

        // Find the corresponding GenerateTestInfo node to check if there are actual tests
        let meta_node = BuildPlanNode::GenerateTestInfo(target);
        let meta_artifacts = match build_meta.artifacts.get(&meta_node) {
            Some(a) => a,
            None => continue,
        };

        // Read test info to check if there are actual tests
        let meta_path = &meta_artifacts.artifacts[1]; // Second artifact is the JSON
        let meta_file = match std::fs::File::open(meta_path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let meta: MooncGenTestInfo = match serde_json_lenient::from_reader(meta_file) {
            Ok(m) => m,
            Err(_) => continue,
        };

        // Skip if no tests exist (legacy behavior)
        if meta.no_args_tests.values().all(|v| v.is_empty()) {
            continue;
        }

        let executable_path = &node_artifacts.artifacts[0];

        // For JS backend, create .cjs wrapper file (matching legacy behavior)
        if matches!(build_meta.target_backend, RunBackend::Js) {
            let wrapper_path = executable_path.with_extension("cjs");

            let js_driver_template = include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../moonbuild/template/test_driver/js_driver.js"
            ));

            // Create wrapper pointing to the JS file
            let js_driver = js_driver_template.replace(
                "origin_js_path",
                &executable_path.display().to_string().replace('\\', "/"),
            );

            std::fs::write(&wrapper_path, &js_driver).with_context(|| {
                format!("Failed to write JS wrapper at {}", wrapper_path.display())
            })?;

            // Write package.json to prevent node from using outer "type": "module"
            let _ = std::fs::write(target_dir.join("package.json"), "{}");
            if let Some(parent) = executable_path.parent() {
                let _ = std::fs::write(parent.join("package.json"), "{}");
            }

            artifacts_path.push(wrapper_path);
        } else {
            artifacts_path.push(executable_path.clone());
        }
    }

    Ok(TestArtifacts { artifacts_path })
}
