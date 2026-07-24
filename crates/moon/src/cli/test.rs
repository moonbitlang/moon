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

use crate::cli::profile;
use crate::filter::TargetPackageGroup;
use crate::filter::canonicalize_with_filename;
use crate::filter::ensure_packages_support_backend;
use crate::filter::filter_pkg_by_dir;
use crate::filter::format_supported_backends;
use crate::filter::group_packages_by_preferred_backend;
use crate::filter::match_packages_with_fuzzy;
use crate::filter::package_supports_backend;
use crate::filter::select_packages;
use crate::rr_build;
use crate::rr_build::preconfig_compile;
use crate::rr_build::{BuildConfig, CalcUserIntentOutput};
use crate::run::collect_test_outline;
use crate::run::perform_promotion;
use crate::run::{TestFilter, TestIndex, TestOutlineEntry};
use anyhow::Context;
use anyhow::bail;
use clap::builder::ArgPredicate;
use colored::Colorize;
use moonbuild_rupes_recta::build_plan::InputDirective;
use moonbuild_rupes_recta::intent::UserIntent;
use moonbuild_rupes_recta::model::BuildPlanNode;
use moonbuild_rupes_recta::model::BuildTarget;
use moonbuild_rupes_recta::model::PackageId;
use moonutil::build_options::{RunMode, TestArtifacts, TestIndexRange};
use moonutil::cli_support::AutoSyncFlags;
use moonutil::command_output::CommandOutput;
use moonutil::locks::FileLock;
use moonutil::project::{PackageDirs, ProjectProbe};
use moonutil::target::{SurfaceTarget, TargetBackend, lower_surface_targets};
use moonutil::user_log::UserLog;
use std::path::{Path, PathBuf};
use tracing::{Level, debug, info, instrument, trace};

use super::BenchSubcommand;
use super::{BuildFlags, UniversalFlags};

struct ResolvedTestPathFilter {
    package: PackageId,
    filename: Option<String>,
}

enum ResolvedTestSelection {
    Workspace {
        packages: Vec<PackageId>,
    },
    Packages {
        packages: Vec<PackageId>,
    },
    Paths {
        filters: Vec<ResolvedTestPathFilter>,
    },
}

impl ResolvedTestSelection {
    fn to_runtime_filter(
        &self,
        resolve_output: &moonbuild_rupes_recta::ResolveOutput,
        cmd: &TestLikeSubcommand<'_>,
    ) -> Result<TestFilter, anyhow::Error> {
        let mut filter = TestFilter {
            name_filter: cmd.filter.clone(),
            ..Default::default()
        };

        match self {
            ResolvedTestSelection::Workspace { .. } => {}
            ResolvedTestSelection::Packages { packages } => {
                if let [pkg_id] = packages.as_slice() {
                    filter.add_autodetermine_target(
                        *pkg_id,
                        cmd.file.as_deref(),
                        selected_test_index(cmd)?,
                    );
                } else if packages.len() > 1 {
                    if cmd.file.is_some() || cmd.index.is_some() || cmd.doc_index.is_some() {
                        bail!(
                            "Cannot filter by file or index when multiple packages are specified. Matched packages: {:?}",
                            package_names(resolve_output, packages)
                        );
                    }
                    for &pkg_id in packages {
                        filter.add_autodetermine_target(pkg_id, None, None);
                    }
                }
            }
            ResolvedTestSelection::Paths { filters } => {
                let test_index = selected_test_index(cmd)?;
                for path_filter in filters {
                    filter.add_autodetermine_target(
                        path_filter.package,
                        path_filter.filename.as_deref(),
                        test_index,
                    );
                }
            }
        };

        Ok(filter)
    }

    fn to_build_intent(
        &self,
        resolve_output: &moonbuild_rupes_recta::ResolveOutput,
        cmd: &TestLikeSubcommand<'_>,
        target_backend: TargetBackend,
        filter: &TestFilter,
    ) -> Result<CalcUserIntentOutput, anyhow::Error> {
        match self {
            ResolvedTestSelection::Workspace { packages } => Ok(packages
                .iter()
                .copied()
                .map(UserIntent::Test)
                .collect::<Vec<_>>()
                .into()),
            ResolvedTestSelection::Packages { .. } | ResolvedTestSelection::Paths { .. } => {
                let directive = match self {
                    ResolvedTestSelection::Packages { packages } => {
                        ensure_packages_support_backend(
                            resolve_output,
                            packages.iter().copied(),
                            target_backend,
                        )?;
                        if let [pkg_id] = packages.as_slice() {
                            let trace_pkg = if cmd.build_flags.enable_value_tracing {
                                Some(*pkg_id)
                            } else {
                                None
                            };
                            rr_build::build_patch_directive_for_package(
                                *pkg_id,
                                false,
                                trace_pkg,
                                cmd.patch_file.as_deref(),
                                true,
                            )
                            .context("failed to build input directive")?
                        } else {
                            if cmd.patch_file.is_some() && packages.len() > 1 {
                                bail!(
                                    "Cannot apply patch file when multiple packages are specified. Matched packages: {:?}",
                                    package_names(resolve_output, packages)
                                );
                            }
                            InputDirective::default()
                        }
                    }
                    ResolvedTestSelection::Paths { .. } => InputDirective::default(),
                    ResolvedTestSelection::Workspace { .. } => unreachable!(),
                };
                Ok((test_intents_from_filter(filter), directive).into())
            }
        }
    }
}

/// Print test summary statistics in the legacy format
fn print_test_summary(
    total: usize,
    passed: usize,
    quiet: bool,
    backend_hint: Option<&str>,
    user_log: &UserLog,
) {
    if total == 0 {
        user_log.warn("no test entry found.");
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

fn print_test_outline(entries: &[TestOutlineEntry], user_log: &UserLog) {
    if entries.is_empty() {
        user_log.warn("no test entry found.");
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
#[derive(Debug, clap::Parser)]
#[clap(group = clap::ArgGroup::new("test_index_selector").multiple(false))]
pub(crate) struct TestSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    /// Run test in the specified package
    #[clap(short, long, num_args(1..))]
    pub package: Option<Vec<String>>,

    /// Run test in the specified file. Only valid when `--package` is also specified.
    #[clap(long, hide = true, requires = "package")]
    pub file: Option<String>,

    /// Run only the index-th test in the file. Accepts a single index or a left-inclusive
    /// right-exclusive range like `0-2`. Only valid when a single file is selected.
    /// Implies `--include-skipped`.
    #[clap(short, long, group = "test_index_selector")]
    pub index: Option<TestIndexRange>,

    /// Run only the index-th doc test in the file. Only valid when a single file is selected.
    /// Implies `--include-skipped`.
    #[clap(long, group = "test_index_selector")]
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
    #[clap(long, conflicts_with = "profile")]
    pub build_only: bool,

    /// Profile native test executables using Time Profiler on macOS or perf on Linux
    #[clap(long, conflicts_with_all = ["build_only", "update", "outline"])]
    pub profile: bool,

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
    #[clap(long = "doc", hide = true)]
    pub doc_test: bool,

    /// Run tests for a filesystem path. If in a project, `PATH` may point to a
    /// package directory or a file inside a package; otherwise, runs in a
    /// temporary project.
    #[clap(conflicts_with_all = ["file", "package"], name="PATH")]
    pub path: Vec<PathBuf>,

    /// Include skipped tests. Automatically implied when `--[doc-]index` is set.
    #[clap(long)]
    #[clap(default_value_if("index", ArgPredicate::IsPresent, "true"))]
    #[clap(default_value_if("doc_index", ArgPredicate::IsPresent, "true"))]
    pub include_skipped: bool,

    /// Run only tests whose name matches the given glob pattern.
    /// Supports '*' (matches any sequence) and '?' (matches any single character).
    #[clap(short = 'f', short_alias = 'F', long)]
    pub filter: Option<String>,
}

#[instrument(skip_all)]
pub(crate) fn run_test(
    cli: UniversalFlags,
    cmd: TestSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let result = run_test_impl(&cli, &cmd, output);
    if crate::run::shutdown_requested() {
        return Ok(130);
    }
    result
}

#[instrument(skip_all)]
fn run_test_impl(
    cli: &UniversalFlags,
    cmd: &TestSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let user_log = output.user_log();
    if cmd.profile {
        profile::ensure_profile_available(profile::MOON_TEST_PROFILE_COMMAND)?;
        ensure_test_profile_target_is_native(&cmd.build_flags)?;
    }

    info!(
        update = cmd.update,
        build_only = cmd.build_only,
        doc_test = cmd.doc_test,
        package_filters = cmd.package.as_ref().map(|p| p.len()).unwrap_or(0),
        path_filters = cmd.path.len(),
        "starting moon test command"
    );
    // Check if we're running within a project
    let mut query = cli.source_tgt_dir.query(cli.workspace_env.clone())?;
    let dirs = match query.probe_project()? {
        ProjectProbe::Found(_) => query.package_dirs()?,
        ProjectProbe::NotFound(not_found) => {
            // Now we're talking about real single-file scenario.
            match cmd.path.as_slice() {
                [path] => {
                    let single_file = cli.source_tgt_dir.single_file_package_dirs(path)?;
                    info!("delegating to single-file test runner");
                    return run_test_in_single_file(
                        cli,
                        cmd,
                        &single_file.file_path,
                        &single_file.package_dirs,
                        output,
                    );
                }
                [] => return Err(not_found.into_error().into()),
                _ => anyhow::bail!("standalone single-file `moon test` expects exactly one `PATH`"),
            }
        }
    };

    debug!(
        source = %dirs.source_dir.display(),
        target = %dirs.target_dir.display(),
        "resolved package directories"
    );

    if cmd.doc_test {
        user_log.warn(
            "--doc flag is deprecated and will be removed in the future, please use `moon test` directly",
        );
    }

    if cmd.build_flags.target.is_empty() {
        debug!("no explicit backend target provided; using defaults");
        let selected_target_backend = cmd.profile.then_some(TargetBackend::Native);
        return run_test_internal(cli, cmd, &dirs, None, selected_target_backend, output);
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
        let x = run_test_internal(cli, cmd, &dirs, display_backend_hint, Some(t), output)
            .context(format!("failed to run test for target {t:?}"))?;
        ret_value = ret_value.max(x);
    }
    debug!(exit_code = ret_value, "completed moon test command");
    Ok(ret_value)
}

fn ensure_test_profile_target_is_native(build_flags: &BuildFlags) -> anyhow::Result<()> {
    if !build_flags.target.is_empty()
        && build_flags.resolve_single_target_backend()? != Some(TargetBackend::Native)
    {
        bail!(
            "{} currently supports only `--target native`",
            profile::MOON_TEST_PROFILE_COMMAND
        );
    }
    Ok(())
}

fn effective_test_build_flags(build_flags: &BuildFlags, profile: bool) -> BuildFlags {
    let mut build_flags = BuildFlags {
        no_strip: !build_flags.strip && !build_flags.release,
        ..build_flags.clone()
    };

    if profile {
        // Native profilers record a native process. Use release-with-symbols by
        // default so the resulting samples are useful without extra flags.
        build_flags.target = vec![SurfaceTarget::Native];
        if !build_flags.debug && !build_flags.release {
            build_flags.release = true;
        }
        if !build_flags.strip && !build_flags.no_strip {
            build_flags.no_strip = true;
        }
    }

    build_flags
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
fn run_test_internal(
    cli: &UniversalFlags,
    cmd: &TestSubcommand,
    dirs: &PackageDirs,
    display_backend_hint: Option<()>,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    debug!(
        backend = ?selected_target_backend,
        build_only = cmd.build_only,
        "entering run_test_internal"
    );
    let exit_code = run_test_or_bench_internal(
        cli,
        cmd.into(),
        dirs,
        display_backend_hint,
        selected_target_backend,
        output,
    )?;
    trace!(exit_code, "run_test_internal finished");
    Ok(exit_code)
}

#[instrument(level = Level::DEBUG, skip_all)]
fn run_test_in_single_file(
    cli: &UniversalFlags,
    cmd: &TestSubcommand,
    single_file_path: &Path,
    dirs: &PackageDirs,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    if cmd.outline && cli.dry_run {
        anyhow::bail!("`--outline` cannot be used with `--dry-run`");
    }
    run_test_in_single_file_rr(cli, cmd, single_file_path, dirs, output)
}

#[instrument(level = Level::DEBUG, skip_all)]
fn run_test_in_single_file_rr(
    cli: &UniversalFlags,
    cmd: &TestSubcommand,
    single_file_path: &Path,
    dirs: &PackageDirs,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let user_log = output.user_log();
    let PackageDirs {
        source_dir,
        target_dir,
        mooncake_bin_dir,
        ..
    } = dirs;
    std::fs::create_dir_all(target_dir)
        .context("failed to create target directory for single-file test")?;

    let mut filter = TestFilter {
        name_filter: cmd.filter.clone(),
        ..Default::default()
    };

    // Resolve synthesized single-file project
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        false,
        cmd.build_flags.enable_coverage,
        cli.workspace_env.clone(),
    );
    let (resolved, backend) = moonbuild_rupes_recta::resolve::resolve_single_file_project(
        &resolve_cfg,
        dirs,
        single_file_path,
        false,
        user_log,
    )?;
    let selected_target_backend = if cmd.profile {
        Some(TargetBackend::Native)
    } else {
        cmd.build_flags.resolve_single_target_backend()?.or(backend)
    };

    let build_flags = effective_test_build_flags(&cmd.build_flags, cmd.profile);
    let mut preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &build_flags,
        selected_target_backend,
        target_dir,
        RunMode::Test,
    );
    // Enable tcc-run to match legacy debug test graph shape
    preconfig.try_tcc_run = !cmd.profile;

    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        user_log,
        &resolved,
    )?;
    let pkg = rr_build::local_packages(&resolved)
        .next()
        .expect("Single-file project must synthesize exactly one package");

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
    let directive = rr_build::build_patch_directive_for_package(pkg, false, trace_pkg, None, true)?;
    let intent = (vec![UserIntent::Test(pkg)], directive).into();
    let (build_meta, build_graph) = rr_build::plan_resolved_build_from_intent(
        preconfig,
        &cli.unstable_feature,
        user_log,
        planning_context,
        intent,
        mooncake_bin_dir,
        resolved,
    )?;

    let test_cmd: TestLikeSubcommand<'_> = cmd.into();
    rr_test_from_plan(
        cli,
        &test_cmd,
        source_dir,
        target_dir,
        None,
        &build_meta,
        build_graph,
        filter,
        None,
        output,
    )
}

pub(crate) struct TestLikeSubcommand<'a> {
    pub run_mode: RunMode,
    pub build_flags: &'a BuildFlags,
    /// Explicit filesystem path filters from positional `PATH` arguments.
    pub explicit_path_filters: &'a [PathBuf],
    pub package: &'a Option<Vec<String>>,
    pub file: &'a Option<String>,
    pub index: &'a Option<TestIndexRange>,
    pub doc_index: &'a Option<u32>,
    pub update: bool,
    pub update_limit: u32,
    pub auto_sync_flags: &'a AutoSyncFlags,
    pub build_only: bool,
    pub profile: bool,
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
            explicit_path_filters: &cmd.path,
            file: &cmd.file,
            index: &cmd.index,
            doc_index: &cmd.doc_index,
            update: cmd.update,
            update_limit: cmd.limit,
            auto_sync_flags: &cmd.auto_sync_flags,
            build_only: cmd.build_only,
            profile: cmd.profile,
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
            explicit_path_filters: &cmd.path,
            package: &cmd.package,
            file: &cmd.file,
            index: &cmd.index,
            doc_index: &None,
            update: false,
            update_limit: 256, // FIXME: unsure about why this default, shouldn't bench have only 1 run?
            auto_sync_flags: &cmd.auto_sync_flags,
            build_only: cmd.build_only,
            profile: false,
            no_parallelize: cmd.no_parallelize,
            outline: false,
            test_failure_json: false,
            patch_file: &None,
            include_skipped: false,
            filter: &None,
        }
    }
}

pub(crate) fn plan_test_or_bench_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &TestLikeSubcommand<'_>,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    user_log: &UserLog,
) -> Result<(rr_build::BuildMeta, rr_build::BuildInput, TestFilter), anyhow::Error> {
    // Keep the planning flow explicit:
    // 1. derive the effective build flags used by test/bench,
    // 2. build the compile preconfig,
    // 3. let RR turn resolved packages plus user intent into a graph and filter.
    let build_flags = effective_test_build_flags(cmd.build_flags, cmd.profile);
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

    // Match the legacy dry-run graph shape for `moon test`.
    if cmd.run_mode != RunMode::Bench && !cmd.profile {
        preconfig.try_tcc_run = true;
    }

    let mut filter = TestFilter {
        name_filter: cmd.filter.clone(),
        ..Default::default()
    };
    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        user_log,
        &resolve_output,
    )?;
    let intent = calc_user_intent(
        &resolve_output,
        cmd,
        &mut filter,
        planning_context.target_backend(),
        user_log,
    )?;
    let (build_meta, build_graph) = rr_build::plan_resolved_build_from_intent(
        preconfig,
        &cli.unstable_feature,
        user_log,
        planning_context,
        intent,
        mooncake_bin_dir,
        resolve_output,
    )?;
    Ok((build_meta, build_graph, filter))
}

pub(crate) fn plan_test_or_bench_rr_from_resolved_all(
    cli: &UniversalFlags,
    cmd: &TestLikeSubcommand<'_>,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    user_log: &UserLog,
) -> Result<Vec<(rr_build::BuildMeta, rr_build::BuildInput, TestFilter)>, anyhow::Error> {
    if let Some(target_backend) = selected_target_backend {
        return plan_test_or_bench_rr_from_resolved(
            cli,
            cmd,
            target_dir,
            mooncake_bin_dir,
            Some(target_backend),
            resolve_output,
            user_log,
        )
        .map(|plan| vec![plan]);
    }

    validate_original_package_selection_filters(&resolve_output, cmd)?;
    let selections = resolve_test_target_selections(&resolve_output, cmd, user_log)?;

    if has_explicit_test_selector(cmd) {
        if selections.is_empty() {
            return plan_test_or_bench_rr_from_resolved(
                cli,
                cmd,
                target_dir,
                mooncake_bin_dir,
                None,
                resolve_output,
                user_log,
            )
            .map(|plan| vec![plan]);
        }

        return selections
            .into_iter()
            .map(|selection| {
                let resolved_selection =
                    resolve_scoped_test_selection(cmd, &resolve_output, selection.packages)?;
                plan_test_or_bench_rr_from_resolved_scoped(
                    cli,
                    cmd,
                    target_dir,
                    mooncake_bin_dir,
                    selection.target_backend,
                    resolve_output.clone(),
                    resolved_selection,
                    user_log,
                )
            })
            .collect();
    }

    if selections.is_empty() {
        return plan_test_or_bench_rr_from_resolved(
            cli,
            cmd,
            target_dir,
            mooncake_bin_dir,
            None,
            resolve_output,
            user_log,
        )
        .map(|plan| vec![plan]);
    }

    selections
        .into_iter()
        .map(|selection| {
            let resolved_selection =
                resolve_scoped_test_selection(cmd, &resolve_output, selection.packages)?;
            plan_test_or_bench_rr_from_resolved_scoped(
                cli,
                cmd,
                target_dir,
                mooncake_bin_dir,
                selection.target_backend,
                resolve_output.clone(),
                resolved_selection,
                user_log,
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn plan_test_or_bench_rr_from_resolved_scoped(
    cli: &UniversalFlags,
    cmd: &TestLikeSubcommand<'_>,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    target_backend: TargetBackend,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    resolved_selection: ResolvedTestSelection,
    user_log: &UserLog,
) -> Result<(rr_build::BuildMeta, rr_build::BuildInput, TestFilter), anyhow::Error> {
    let build_flags = BuildFlags {
        no_strip: !cmd.build_flags.strip && !cmd.build_flags.release,
        ..cmd.build_flags.clone()
    };
    let mut preconfig = preconfig_compile(
        cmd.auto_sync_flags,
        cli,
        &build_flags,
        Some(target_backend),
        target_dir,
        if cmd.run_mode == RunMode::Bench {
            RunMode::Bench
        } else {
            RunMode::Test
        },
    );

    if cmd.run_mode != RunMode::Bench {
        preconfig.try_tcc_run = true;
    }

    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        user_log,
        &resolve_output,
    )?;
    debug_assert_eq!(planning_context.target_backend(), target_backend);
    let filter = resolved_selection.to_runtime_filter(&resolve_output, cmd)?;
    let intent = resolved_selection.to_build_intent(
        &resolve_output,
        cmd,
        planning_context.target_backend(),
        &filter,
    )?;
    let (build_meta, build_graph) = rr_build::plan_resolved_build_from_intent(
        preconfig,
        &cli.unstable_feature,
        user_log,
        planning_context,
        intent,
        mooncake_bin_dir,
        resolve_output,
    )?;
    Ok((build_meta, build_graph, filter))
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_test_or_bench_internal(
    cli: &UniversalFlags,
    cmd: TestLikeSubcommand,
    dirs: &PackageDirs,
    display_backend_hint: Option<()>,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    debug!(
        run_mode = ?cmd.run_mode,
        update = cmd.update,
        build_only = cmd.build_only,
        package_filters = cmd.package.as_ref().map(|p| p.len()).unwrap_or(0),
        path_filters = cmd.explicit_path_filters.len(),
        "entering run_test_or_bench_internal"
    );
    trace!(
        index = ?cmd.index,
        doc_index = cmd.doc_index,
        no_parallelize = cmd.no_parallelize,
        "cli filter state"
    );

    // Accept -i/--doc-index when either --file is set or the positional PATH refers to a file.
    // explicit_is_file is true only when PATH is an existing regular file.
    let explicit_is_file = matches!(cmd.explicit_path_filters, [path] if path.is_file());

    if cmd.package.is_none() && cmd.file.is_some() {
        anyhow::bail!("`--file` must be used with `--package`");
    }
    if cmd.explicit_path_filters.len() > 1 && (cmd.index.is_some() || cmd.doc_index.is_some()) {
        anyhow::bail!("`--index` and `--doc-index` cannot be used with multiple `PATH`s");
    }
    if cmd.file.is_none() && cmd.index.is_some() && !explicit_is_file {
        anyhow::bail!("`--index` must be used with a single file selected by `--file` or `PATH`");
    }
    if cmd.file.is_none() && cmd.doc_index.is_some() && !explicit_is_file {
        anyhow::bail!(
            "`--doc-index` must be used with a single file selected by `--file` or `PATH`"
        );
    }
    if !cmd.explicit_path_filters.is_empty() && (cmd.package.is_some() || cmd.file.is_some()) {
        anyhow::bail!("cannot combine positional `PATH` filters with `--package` or `--file`");
    }
    if cmd.outline && cli.dry_run {
        anyhow::bail!("`--outline` cannot be used with `--dry-run`");
    }
    if cmd.profile && cmd.run_mode != RunMode::Test {
        anyhow::bail!("`--profile` is only supported for `moon test`");
    }

    debug!("selecting test runner implementation");
    run_test_rr(
        cli,
        &cmd,
        dirs,
        display_backend_hint,
        selected_target_backend,
        output,
    )
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
fn run_test_rr(
    cli: &UniversalFlags,
    cmd: &TestLikeSubcommand<'_>,
    dirs: &PackageDirs,
    display_backend_hint: Option<()>, // FIXME: unsure why it's option but as-is for now
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
) -> Result<i32, anyhow::Error> {
    let user_log = output.user_log();
    let PackageDirs {
        source_dir,
        target_dir,
        mooncake_bin_dir,
        ..
    } = dirs;
    info!(run_mode = ?cmd.run_mode, update = cmd.update, build_only = cmd.build_only, "starting rupes-recta test run");
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new_with_load_defaults(
        cmd.auto_sync_flags.frozen,
        !cmd.build_flags.std(),
        cmd.build_flags.enable_coverage,
        cli.workspace_env.clone(),
    );
    let synced_env = moonbuild_rupes_recta::sync_dependencies(&resolve_cfg, dirs)?;
    let resolve_output =
        moonbuild_rupes_recta::resolve_synced_project(&resolve_cfg, synced_env, user_log)?;
    let planned_runs = plan_test_or_bench_rr_from_resolved_all(
        cli,
        cmd,
        target_dir,
        mooncake_bin_dir,
        selected_target_backend,
        resolve_output,
        user_log,
    )?;
    let effective_display_backend_hint = if planned_runs.len() > 1 {
        Some(())
    } else {
        display_backend_hint
    };

    let mut build_only_artifacts = cmd.build_only.then_some(TestArtifacts {
        artifacts_path: Vec::new(),
        test_filter_args: Vec::new(),
    });
    let mut exit_code = 0;
    for (build_meta, build_graph, filter) in planned_runs {
        debug!(
            artifact_count = build_meta.artifacts.len(),
            backend = ?build_meta.target_backend,
            "planned rupes-recta build graph"
        );

        exit_code = exit_code.max(rr_test_from_plan(
            cli,
            cmd,
            source_dir,
            target_dir,
            effective_display_backend_hint,
            &build_meta,
            build_graph,
            filter,
            build_only_artifacts.as_mut(),
            output,
        )?);
    }
    if !cli.dry_run
        && let Some(test_artifacts) = build_only_artifacts
    {
        println!("{}", serde_json_lenient::to_string(&test_artifacts)?);
    }
    Ok(exit_code)
}

/// The nodes wanted to run a test for a build target
fn node_from_target(x: BuildTarget) -> [BuildPlanNode; 2] {
    [
        BuildPlanNode::make_executable(x),
        BuildPlanNode::generate_test_info(x),
    ]
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
    target_backend: TargetBackend,
    out_filter: &mut TestFilter,
    user_log: &UserLog,
) -> Result<InputDirective, anyhow::Error> {
    let package_matches = match_packages_with_fuzzy(
        resolve_output,
        affected_packages.iter().copied(),
        package_filter,
    );
    let filtered_package_ids = package_matches.matched;
    ensure_packages_support_backend(
        resolve_output,
        filtered_package_ids.iter().copied(),
        target_backend,
    )?;
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
        user_log.warn(format!(
            "package `{}` not found, make sure you have spelled it correctly, e.g. `moonbitlang/core/hashmap`(exact match) or `hashmap`(fuzzy match)",
            package_filter.join(", ")
        ));
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
    target_backend: moonutil::target::TargetBackend,
    user_log: &UserLog,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let all_affected_packages: Vec<_> = resolve_output
        .local_modules()
        .iter()
        .flat_map(|&module_id| {
            resolve_output
                .pkg_dirs
                .packages_for_module(module_id)
                .into_iter()
                .flat_map(|packages| packages.values().copied())
        })
        .collect();
    calc_user_intent_from_packages(
        resolve_output,
        cmd,
        out_filter,
        &all_affected_packages,
        target_backend,
        user_log,
        cmd.explicit_path_filters,
        cmd.package.as_deref(),
    )
}

#[allow(clippy::too_many_arguments)]
fn calc_user_intent_from_packages(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &TestLikeSubcommand<'_>,
    out_filter: &mut TestFilter,
    all_affected_packages: &[PackageId],
    target_backend: moonutil::target::TargetBackend,
    user_log: &UserLog,
    explicit_path_filters: &[PathBuf],
    package_filter: Option<&[String]>,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    debug!(
        package_count = all_affected_packages.len(),
        module_count = resolve_output.local_modules().len(),
        "calculating user intent for workspace"
    );
    let backend_affected_packages = all_affected_packages
        .iter()
        .copied()
        .filter(|&pkg_id| package_supports_backend(resolve_output, pkg_id, target_backend))
        .collect::<Vec<_>>();

    let directive = if !explicit_path_filters.is_empty() {
        let test_index = if let Some(index) = cmd.index {
            Some(TestIndex::Regular(*index))
        } else if let Some(id) = cmd.doc_index {
            Some(TestIndex::DocTest(TestIndexRange::from_single(*id)?))
        } else {
            None
        };

        if let [path] = explicit_path_filters {
            let (dir, filename) = canonicalize_with_filename(path)?;
            let pkg = filter_pkg_by_dir(resolve_output, &dir)?;

            if !package_supports_backend(resolve_output, pkg, target_backend) {
                ensure_packages_support_backend(resolve_output, [pkg], target_backend)?;
            }

            out_filter.add_autodetermine_target(pkg, filename.as_deref(), test_index);
            trace!("single explicit path filter applied");
        } else {
            let mut unsupported_paths = Vec::new();
            let mut supported_paths = 0;

            for path in explicit_path_filters {
                let (dir, filename) = canonicalize_with_filename(path)?;
                debug!(dir = %dir.display(), filename = ?filename, "resolved explicit path filter");

                let Ok(pkg) = filter_pkg_by_dir(resolve_output, &dir) else {
                    user_log.warn(format!(
                        "skipping path `{}` because it is not a package in the current work context.",
                        path.display()
                    ));
                    continue;
                };

                if !package_supports_backend(resolve_output, pkg, target_backend) {
                    unsupported_paths.push((path, pkg));
                    continue;
                }

                supported_paths += 1;
                out_filter.add_autodetermine_target(pkg, filename.as_deref(), test_index);
            }

            if supported_paths == 0 && !unsupported_paths.is_empty() {
                let mut unsupported_packages = Vec::new();
                for (_, pkg_id) in &unsupported_paths {
                    if !unsupported_packages.contains(pkg_id) {
                        unsupported_packages.push(*pkg_id);
                    }
                }
                ensure_packages_support_backend(
                    resolve_output,
                    unsupported_packages,
                    target_backend,
                )?;
            }

            for (path, pkg_id) in unsupported_paths {
                let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
                user_log.warn(format!(
                    "skipping path `{}` because package `{}` does not support target backend `{}`. Supported backends: {}",
                    path.display(),
                    pkg.fqn,
                    target_backend,
                    format_supported_backends(resolve_output, pkg_id),
                ));
            }

            trace!("explicit path filters applied");
        }
        Default::default()
    } else if let Some(package_filter) = package_filter {
        let value_tracing = cmd.build_flags.enable_value_tracing;
        apply_list_of_filters(
            all_affected_packages,
            resolve_output,
            package_filter,
            cmd.file.as_deref(),
            *cmd.index,
            *cmd.doc_index,
            cmd.patch_file.as_deref(),
            value_tracing,
            target_backend,
            out_filter,
            user_log,
        )?
    } else {
        // No filter: emit one intent per package (Test/Bench)
        let intents: Vec<_> = backend_affected_packages
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

fn resolve_scoped_test_selection(
    cmd: &TestLikeSubcommand<'_>,
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    scoped_packages: Vec<PackageId>,
) -> anyhow::Result<ResolvedTestSelection> {
    if !cmd.explicit_path_filters.is_empty() {
        let mut filters = Vec::new();
        for path in cmd.explicit_path_filters {
            let Ok((dir, filename)) = canonicalize_with_filename(path) else {
                continue;
            };
            let Ok(package) = filter_pkg_by_dir(resolve_output, &dir) else {
                continue;
            };
            if scoped_packages.contains(&package) {
                filters.push(ResolvedTestPathFilter { package, filename });
            }
        }
        return Ok(ResolvedTestSelection::Paths { filters });
    }

    if cmd.package.is_some() {
        return Ok(ResolvedTestSelection::Packages {
            packages: scoped_packages,
        });
    }

    Ok(ResolvedTestSelection::Workspace {
        packages: scoped_packages,
    })
}

fn selected_test_index(cmd: &TestLikeSubcommand<'_>) -> anyhow::Result<Option<TestIndex>> {
    if let Some(index) = cmd.index {
        Ok(Some(TestIndex::Regular(*index)))
    } else if let Some(id) = cmd.doc_index {
        Ok(Some(TestIndex::DocTest(TestIndexRange::from_single(*id)?)))
    } else {
        Ok(None)
    }
}

fn package_names(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    packages: &[PackageId],
) -> Vec<String> {
    packages
        .iter()
        .map(|id| resolve_output.pkg_dirs.get_package(*id).fqn.to_string())
        .collect()
}

fn test_intents_from_filter(filter: &TestFilter) -> Vec<UserIntent> {
    let Some(filt) = filter.filter.as_ref() else {
        return vec![];
    };

    let mut packages = Vec::new();
    for (target, _) in &filt.0 {
        if !packages.contains(&target.package) {
            packages.push(target.package);
        }
    }
    packages.into_iter().map(UserIntent::Test).collect()
}

fn has_explicit_test_selector(cmd: &TestLikeSubcommand<'_>) -> bool {
    !cmd.explicit_path_filters.is_empty() || cmd.package.is_some()
}

fn resolve_test_target_selections(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &TestLikeSubcommand<'_>,
    user_log: &UserLog,
) -> anyhow::Result<Vec<TargetPackageGroup>> {
    let selected = resolve_selected_test_packages(resolve_output, cmd, user_log)?;
    let mut selections = group_packages_by_preferred_backend(resolve_output, selected);

    for selection in &mut selections {
        selection.packages = selection
            .packages
            .iter()
            .copied()
            .filter(|&pkg| package_supports_backend(resolve_output, pkg, selection.target_backend))
            .collect();
    }
    selections.retain(|selection| !selection.packages.is_empty());

    Ok(selections)
}

fn resolve_selected_test_packages(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &TestLikeSubcommand<'_>,
    user_log: &UserLog,
) -> anyhow::Result<Vec<PackageId>> {
    if !cmd.explicit_path_filters.is_empty() {
        return Ok(select_packages(cmd.explicit_path_filters, user_log, |dir| {
            filter_pkg_by_dir(resolve_output, dir)
        })?
        .into_iter()
        .map(|(_, pkg_id)| pkg_id)
        .collect());
    }

    if let Some(package_filter) = cmd.package.as_deref() {
        let all_affected_packages: Vec<_> = resolve_output
            .local_modules()
            .iter()
            .flat_map(|&module_id| {
                resolve_output
                    .pkg_dirs
                    .packages_for_module(module_id)
                    .into_iter()
                    .flat_map(|packages| packages.values().copied())
            })
            .collect();
        return Ok(match_packages_with_fuzzy(
            resolve_output,
            all_affected_packages,
            package_filter,
        )
        .matched);
    }

    Ok(resolve_output
        .local_modules()
        .iter()
        .flat_map(|&module_id| {
            resolve_output
                .pkg_dirs
                .packages_for_module(module_id)
                .into_iter()
                .flat_map(|packages| packages.values().copied())
        })
        .collect())
}

fn validate_original_package_selection_filters(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &TestLikeSubcommand<'_>,
) -> anyhow::Result<()> {
    let Some(package_filter) = cmd.package.as_deref() else {
        return Ok(());
    };

    if cmd.file.is_none()
        && cmd.index.is_none()
        && cmd.doc_index.is_none()
        && cmd.patch_file.is_none()
    {
        return Ok(());
    }

    let matched_packages = match_packages_with_fuzzy(
        resolve_output,
        resolve_output
            .local_modules()
            .iter()
            .flat_map(|&module_id| {
                resolve_output
                    .pkg_dirs
                    .packages_for_module(module_id)
                    .into_iter()
                    .flat_map(|packages| packages.values().copied())
            }),
        package_filter,
    )
    .matched;

    if matched_packages.len() <= 1 {
        return Ok(());
    }

    let package_names = || {
        matched_packages
            .iter()
            .map(|id| resolve_output.pkg_dirs.get_package(*id).fqn.to_string())
            .collect::<Vec<_>>()
    };

    if cmd.file.is_some() || cmd.index.is_some() || cmd.doc_index.is_some() {
        bail!(
            "Cannot filter by file or index when multiple packages are specified. Matched packages: {:?}",
            package_names()
        );
    }
    if cmd.patch_file.is_some() {
        bail!(
            "Cannot apply patch file when multiple packages are specified. Matched packages: {:?}",
            package_names()
        );
    }

    Ok(())
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
    build_only_artifacts: Option<&mut TestArtifacts>,
    output: &CommandOutput,
) -> Result<i32, anyhow::Error> {
    let user_log = output.user_log();
    let built = match execute_test_build_from_plan(
        cli,
        cmd,
        source_dir,
        target_dir,
        build_meta,
        build_graph,
        output,
    )? {
        TestBuildExecution::DryRun => return Ok(0),
        TestBuildExecution::BuildFailed(exit_code) => return Ok(exit_code),
        TestBuildExecution::Built(built) => *built,
    };

    if cmd.outline {
        let entries = collect_test_outline(
            build_meta,
            &filter,
            cmd.include_skipped,
            cmd.run_mode == RunMode::Bench,
        )?;
        print_test_outline(&entries, user_log);
        return Ok(0);
    }

    if cmd.build_only {
        // Match legacy behavior: create JS wrappers and print test artifacts as JSON
        let test_artifacts = collect_test_artifacts_for_build_only(
            build_meta,
            target_dir,
            &filter,
            cmd.include_skipped,
            cmd.run_mode == RunMode::Bench,
        )?;
        if let Some(artifacts) = build_only_artifacts {
            artifacts
                .artifacts_path
                .extend(test_artifacts.artifacts_path);
            artifacts
                .test_filter_args
                .extend(test_artifacts.test_filter_args);
        } else {
            println!("{}", serde_json_lenient::to_string(&test_artifacts)?);
        }
        return Ok(0);
    }

    if cmd.profile {
        return profile::profile_test_invocations(
            cli,
            source_dir,
            target_dir,
            build_meta,
            &filter,
            cmd.include_skipped,
            output,
        );
    }

    let mut test_result = crate::run::run_tests(
        build_meta,
        source_dir,
        target_dir,
        &filter,
        cmd.include_skipped,
        cmd.run_mode == RunMode::Bench,
        cmd.no_parallelize,
        cmd.build_flags.jobs,
        user_log,
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
            if loop_count >= cmd.update_limit {
                user_log.warn(format!(
                    "reached the limit of {} update passes, stopping further updates.",
                    cmd.update_limit
                ));
                break;
            }
            loop_count += 1;

            // Get the graph from backup
            let build_graph = built
                .build_graph_backup
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
                &built.build_config,
                build_graph,
                target_dir,
                Some(build_meta),
                user_log,
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
                cmd.no_parallelize,
                cmd.build_flags.jobs,
                user_log,
            )?;
            let _rerun_summary = new_test_result.summary();

            // Merge test results
            test_result.merge(&new_test_result);
            last_test_result = Some(new_test_result);
        }
    }

    test_result.print_result(build_meta, cli.verbose, cmd.test_failure_json);
    let summary = test_result.summary();
    print_test_summary(
        summary.total,
        summary.passed,
        cli.quiet,
        backend_hint,
        user_log,
    );

    if summary.total == summary.passed {
        Ok(0)
    } else {
        Ok(2)
    }
}

struct BuiltTestExecution {
    _lock: FileLock,
    build_config: BuildConfig,
    build_graph_backup: Option<rr_build::BuildInput>,
}

enum TestBuildExecution {
    DryRun,
    BuildFailed(i32),
    Built(Box<BuiltTestExecution>),
}

#[allow(clippy::too_many_arguments)]
fn execute_test_build_from_plan(
    cli: &UniversalFlags,
    cmd: &TestLikeSubcommand<'_>,
    source_dir: &Path,
    target_dir: &Path,
    build_meta: &rr_build::BuildMeta,
    build_graph: rr_build::BuildInput,
    output: &CommandOutput,
) -> Result<TestBuildExecution, anyhow::Error> {
    let user_log = output.user_log();
    if cli.dry_run {
        output.write_result(|writer| {
            rr_build::write_dry_run(
                writer,
                &build_graph,
                build_meta.artifacts.values(),
                source_dir,
                target_dir,
            )
        })?;
        // Test command lines depend on generated metadata, which dry-run does
        // not materialize. Profile dry-run therefore stops at the build graph.
        return Ok(TestBuildExecution::DryRun);
    }

    let lock = FileLock::lock(target_dir)?;
    // Generate the all_pkgs.json for indirect dependency resolution
    // before executing the build
    rr_build::generate_all_pkgs_json(build_meta)?;

    let build_config = BuildConfig::from_flags(cmd.build_flags, &cli.unstable_feature, cli.verbose);

    // since n2 build consumes the graph, we back it up for reruns
    let build_graph_backup = cmd.update.then(|| build_graph.clone());
    let result =
        rr_build::execute_test_build(&build_config, build_graph, target_dir, build_meta, user_log)?;
    debug!(
        success = result.successful(),
        exit_code = result.return_code_for_success(),
        "executed rupes-recta build"
    );

    if !result.successful() {
        return Ok(TestBuildExecution::BuildFailed(
            result.return_code_for_success(),
        ));
    }

    Ok(TestBuildExecution::Built(Box::new(BuiltTestExecution {
        _lock: lock,
        build_config,
        build_graph_backup,
    })))
}

/// Collect test artifacts for --build-only mode, matching legacy behavior.
/// For JS backend, creates .cjs wrapper files and returns those paths.
/// For other backends, returns the executable paths directly.
/// Only includes artifacts that have actual tests (skips empty test executables).
fn collect_test_artifacts_for_build_only(
    build_meta: &rr_build::BuildMeta,
    target_dir: &Path,
    filter: &TestFilter,
    include_skipped: bool,
    bench: bool,
) -> anyhow::Result<TestArtifacts> {
    use moonbuild_rupes_recta::model::RunBackend;

    let mut artifacts_path = vec![];
    let mut test_filter_args = vec![];

    for invocation in
        crate::run::collect_test_invocations(build_meta, filter, include_skipped, bench)?
    {
        // For JS backend, create .cjs wrapper file (matching legacy behavior)
        if matches!(build_meta.target_backend, RunBackend::Js) {
            // Write package.json to prevent node from using outer "type": "module"
            let _ = std::fs::write(target_dir.join("package.json"), "{}");
            if let Some(parent) = invocation.executable.parent() {
                let _ = std::fs::write(parent.join("package.json"), "{}");
            }

            let filter_arg = serde_json::to_string(&invocation.args)
                .context("failed to serialize JS test filter args")?;

            artifacts_path.push(invocation.executable);
            test_filter_args.push(filter_arg);
        } else {
            artifacts_path.push(invocation.executable);
        }
    }

    Ok(TestArtifacts {
        artifacts_path,
        test_filter_args,
    })
}
