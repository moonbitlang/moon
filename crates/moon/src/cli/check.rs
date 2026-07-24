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

//! Workflow overview for `moon check`:
//!
//! 1. Parse the CLI selector (`PATH`, `-p`, or whole workspace).
//! 2. Resolve packages and decide which backend(s) to check:
//!    - explicit `--target` keeps one backend;
//!    - otherwise local packages are grouped by
//!      `module preferred -> workspace preferred -> default backend`.
//! 3. `plan_check_rr_from_resolved_all` turns those backend groups into an
//!    ordered list of single-backend RR plans.
//! 4. `plan_check_rr_from_resolved` still plans exactly one backend group.
//! 5. The runtime executes planned runs in order; the last run updates
//!    `packages.json`.
//!
use anyhow::Context;
use log::LevelFilter;
use moonbuild_rupes_recta::intent::UserIntent;
use moonbuild_rupes_recta::model::PackageId;
use moonutil::build_options::RunMode;
use moonutil::cli_support::AutoSyncFlags;
use moonutil::cli_support::UniversalFlags;
use moonutil::command_output::CommandOutput;
use moonutil::constants::WATCH_MODE_DIR;
use moonutil::locks::FileLock;
use moonutil::project::{PackageDirs, ProjectProbe};
use moonutil::target::TargetBackend;
use moonutil::target::lower_surface_targets;
use moonutil::user_log::UserLog;
use std::path::{Path, PathBuf};
use tracing::{Level, instrument};

use crate::filter::{
    TargetPackageGroup, canonicalize_with_filename, ensure_package_supports_backend,
    ensure_packages_support_backend, filter_pkg_by_dir, group_packages_by_preferred_backend,
    package_supports_backend, select_packages, select_supported_packages,
};
use crate::rr_build::{self, BuildConfig, CalcUserIntentOutput, preconfig_compile};
use crate::watch::prebuild_output::{PrebuildWatchPaths, rr_get_prebuild_watch_paths};
use crate::watch::{WatchOutput, watching};

use super::BuildFlags;

#[derive(Debug, Clone)]
struct ResolvedCheckSelection {
    packages: Vec<PackageId>,
    patch_file: Option<PathBuf>,
}

impl ResolvedCheckSelection {
    fn from_command(packages: Vec<PackageId>, cmd: &CheckSubcommand) -> Self {
        Self {
            packages,
            patch_file: cmd.patch_file.clone(),
        }
    }

    fn into_user_intent(self) -> anyhow::Result<CalcUserIntentOutput> {
        let directive =
            build_directive_for_selected_packages(&self.packages, self.patch_file.as_deref())?;
        Ok((
            self.packages.into_iter().map(UserIntent::Check).collect(),
            directive,
        )
            .into())
    }
}

/// Check the current package, but don't build object files
#[derive(Debug, clap::Parser, Clone)]
#[clap(group = clap::ArgGroup::new("package_selector").multiple(false))]
pub(crate) struct CheckSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Monitor the file system and automatically check files
    #[clap(long, short)]
    pub watch: bool,

    /// Legacy package directory path relative to the module source root (`source` in `moon.mod.json`)
    //
    // This selects a package directory under the module source root, not an arbitrary
    // filesystem path. Use positional `PATH` for filesystem paths.
    // TODO: Unify the `-p` flag to specifying package name, see #1139
    #[clap(
        long,
        short_alias = 'p',
        value_name = "PACKAGE_DIR",
        hide = true,
        group = "package_selector"
    )]
    pub package_path: Option<PathBuf>,

    /// The patch file to check. Only valid when the selector resolves to a single package.
    #[clap(long, requires = "package_selector")]
    pub patch_file: Option<PathBuf>,

    /// Whether to explain the error code with details.
    #[clap(long)]
    pub explain: bool,

    /// Filesystem path to a package directory or `.mbt` / `.mbt.md` file
    #[clap(conflicts_with = "watch", name = "PATH", group = "package_selector")]
    pub path: Vec<PathBuf>,

    /// Check whether the code is properly formatted
    #[clap(long)]
    pub fmt: bool,
}

#[instrument(skip_all)]
pub(crate) fn run_check(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let user_log = output.user_log();
    if cmd.fmt {
        let mut cli_for_fmt = cli.clone();
        cli_for_fmt.quiet = true;
        let fmt_output = CommandOutput::new(LevelFilter::Error);
        let fmt_exit_code = crate::cli::fmt::run_fmt(
            &cli_for_fmt,
            crate::cli::FmtSubcommand {
                check: false,
                sort_input: false,
                warn: true,
                path: cmd.path.clone(),
                args: vec![],
            },
            &fmt_output,
        )?;
        if fmt_exit_code != 0 {
            user_log.warn("formatting code failed");
        }
    }

    // Check if we're running within a project
    let mut query = cli.source_tgt_dir.query(cli.workspace_env.clone())?;
    let (mut dirs, single_file) = match query.probe_project()? {
        ProjectProbe::Found(_) => {
            let dirs = query.package_dirs()?;
            (dirs, None)
        }
        ProjectProbe::NotFound(not_found) => {
            // Now we're talking about real single-file scenario.
            match cmd.path.as_slice() {
                [path] => {
                    let single_file = cli.source_tgt_dir.single_file_package_dirs(path)?;
                    (single_file.package_dirs, Some(single_file.file_path))
                }
                [] => return Err(not_found.into_error().into()),
                _ => {
                    anyhow::bail!("standalone single-file `moon check` expects exactly one `PATH`");
                }
            }
        }
    };
    let watch_ignored_subtree = dirs.target_dir.clone();
    if cmd.watch {
        dirs.target_dir = dirs.target_dir.join(WATCH_MODE_DIR);
        dirs.mooncake_bin_dir = dirs.target_dir.join(moonutil::constants::MOON_BIN_DIR);
    }

    if cmd.build_flags.target.is_empty() {
        return run_check_internal(
            cli,
            cmd,
            &dirs,
            &watch_ignored_subtree,
            single_file.as_deref(),
            None,
            output,
        );
    }

    let surface_targets = cmd.build_flags.target.clone();
    let targets = lower_surface_targets(&surface_targets);
    let mut ret_value = 0;
    for t in targets {
        let x = run_check_internal(
            cli,
            cmd,
            &dirs,
            &watch_ignored_subtree,
            single_file.as_deref(),
            Some(t),
            output,
        )
        .context(format!("failed to run check for target {t:?}"))?;
        ret_value = ret_value.max(x);
    }
    Ok(ret_value)
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
fn run_check_internal(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    dirs: &PackageDirs,
    watch_ignored_subtree: &Path,
    single_file: Option<&Path>,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    if let Some(single_file_path) = single_file {
        run_check_for_single_file_rr(
            cli,
            cmd,
            single_file_path,
            dirs,
            selected_target_backend,
            output,
        )
    } else {
        run_check_normal_internal(
            cli,
            cmd,
            dirs,
            watch_ignored_subtree,
            selected_target_backend,
            output,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn run_check_for_single_file_rr(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    single_file_path: &Path,
    dirs: &PackageDirs,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let user_log = output.user_log();
    let PackageDirs {
        source_dir,
        target_dir,
        mooncake_bin_dir,
        ..
    } = dirs;
    if cmd.patch_file.is_some() {
        anyhow::bail!("standalone single-file `moon check` does not support `--patch-file`");
    }

    std::fs::create_dir_all(target_dir).context("failed to create target directory")?;

    // Manually synthesize and resolve single file project
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
    let selected_target_backend = selected_target_backend
        .or(cmd.build_flags.resolve_single_target_backend()?)
        .or(backend);

    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        target_dir,
        RunMode::Check,
    );

    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        user_log,
        &resolved,
    )?;
    let intent = get_user_intents_single_file(&resolved, planning_context.target_backend())?;
    let (build_meta, build_graph) = rr_build::plan_resolved_build_from_intent(
        preconfig,
        &cli.unstable_feature,
        user_log,
        planning_context,
        intent,
        mooncake_bin_dir,
        resolved,
    )
    .context("Failed to calculate build plan")?;

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
        return Ok(0);
    }

    let _lock = FileLock::lock(target_dir).with_context(|| {
        format!(
            "failed to acquire build lock in target directory `{}`",
            target_dir.display()
        )
    })?;

    // Generate all_pkgs.json for indirect dependency resolution
    rr_build::generate_all_pkgs_json(&build_meta)?;
    let filename = single_file_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(String::from);
    rr_build::generate_metadata(
        source_dir,
        target_dir,
        &build_meta,
        &build_graph,
        filename.as_deref(),
    )?;

    let mut cfg = BuildConfig::from_flags(&cmd.build_flags, &cli.unstable_feature, cli.verbose);
    cfg.patch_file = cmd.patch_file.clone();
    cfg.explain_errors |= cmd.explain;

    let result = rr_build::execute_build(&cfg, build_graph, target_dir, user_log)?;
    rr_build::report_build_result(&result, rr_build::BuildOperation::Check, &cfg, output)?;

    Ok(if result.successful() { 0 } else { 1 })
}

fn get_user_intents_single_file(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    _backend: TargetBackend,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let m_packages = resolve_output
        .pkg_dirs
        .packages_for_module(resolve_output.local_modules()[0])
        .context("single-file project must resolve a local module")?;
    let pkg = *m_packages
        .iter()
        .next()
        .context("single-file project must resolve exactly one package")?
        .1;

    Ok(vec![UserIntent::Check(pkg)].into())
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
fn run_check_normal_internal(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    dirs: &PackageDirs,
    watch_ignored_subtree: &Path,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let run_once = || -> anyhow::Result<WatchOutput> {
        run_check_normal_internal_rr(cli, cmd, dirs, cmd.watch, selected_target_backend, output)
    };
    if cmd.watch {
        watching(run_once, &dirs.source_dir, watch_ignored_subtree)
    } else {
        run_once().map(|output| if output.ok { 0 } else { 1 })
    }
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
fn run_check_normal_internal_rr(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    dirs: &PackageDirs,
    watch: bool,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
) -> anyhow::Result<WatchOutput> {
    let user_log = output.user_log();
    let PackageDirs {
        source_dir,
        target_dir,
        mooncake_bin_dir,
        ..
    } = dirs;
    std::fs::create_dir_all(target_dir).with_context(|| {
        format!(
            "Failed to create target directory: '{}'",
            target_dir.display()
        )
    })?;

    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        !cmd.build_flags.std(),
        cmd.build_flags.enable_coverage,
        cli.workspace_env.clone(),
    );
    let synced_env = moonbuild_rupes_recta::sync_dependencies(&resolve_cfg, dirs)
        .context("Failed to calculate build plan")?;
    let resolve_output =
        moonbuild_rupes_recta::resolve_synced_project(&resolve_cfg, synced_env, user_log)
            .context("Failed to calculate build plan")?;
    let prebuild_list = if watch {
        rr_get_prebuild_watch_paths(&resolve_output)
    } else {
        PrebuildWatchPaths {
            ignored_paths: Vec::new(),
            watched_paths: Vec::new(),
        }
    };
    let planned_runs = plan_check_rr_from_resolved_all(
        cli,
        cmd,
        source_dir,
        target_dir,
        mooncake_bin_dir,
        selected_target_backend,
        resolve_output,
        user_log,
    )
    .context("Failed to calculate build plan")?;

    let ok = if cli.dry_run {
        output.write_result(|writer| {
            for (build_meta, build_graph) in planned_runs {
                rr_build::write_dry_run(
                    writer,
                    &build_graph,
                    build_meta.artifacts.values(),
                    source_dir,
                    target_dir,
                )?;
            }
            Ok::<_, std::io::Error>(())
        })?;
        true
    } else {
        let _lock = FileLock::lock(target_dir).with_context(|| {
            format!(
                "failed to acquire build lock in target directory `{}`",
                target_dir.display()
            )
        })?;
        let mut cfg = BuildConfig::from_flags(&cmd.build_flags, &cli.unstable_feature, cli.verbose);
        cfg.patch_file = cmd.patch_file.clone();
        cfg.explain_errors |= cmd.explain;
        let mut ok = true;
        for (build_meta, build_graph) in planned_runs {
            // Generate all_pkgs.json for indirect dependency resolution
            rr_build::generate_all_pkgs_json(&build_meta)?;
            // Generate metadata for IDE. The last executed backend wins.
            rr_build::generate_metadata(source_dir, target_dir, &build_meta, &build_graph, None)?;

            let result = rr_build::execute_build(&cfg, build_graph, target_dir, user_log)?;
            rr_build::report_build_result(&result, rr_build::BuildOperation::Check, &cfg, output)?;
            ok &= result.successful();
        }
        ok
    };

    Ok(WatchOutput {
        ok,
        additional_ignored_paths: prebuild_list.ignored_paths,
        additional_watched_paths: prebuild_list.watched_paths,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_check_rr_from_resolved_all(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    user_log: &UserLog,
) -> anyhow::Result<Vec<(rr_build::BuildMeta, rr_build::BuildInput)>> {
    validate_selector_flags_before_split(
        &resolve_output,
        cmd,
        source_dir,
        selected_target_backend,
        user_log,
    )?;

    let selections = resolve_check_target_selections(
        &resolve_output,
        cmd,
        source_dir,
        selected_target_backend,
        user_log,
    )?;

    if selections.is_empty() {
        return plan_check_rr_from_resolved(
            cli,
            cmd,
            source_dir,
            target_dir,
            mooncake_bin_dir,
            selected_target_backend,
            resolve_output,
            user_log,
        )
        .map(|plan| vec![plan]);
    }

    selections
        .into_iter()
        .map(|selection| {
            // The command adapter has resolved raw CLI selectors into
            // PackageIds. RR planning should use those identities and the
            // bin-dependency launcher directory captured by the command
            // adapter.
            plan_check_rr_from_selection(
                cli,
                cmd,
                target_dir,
                mooncake_bin_dir,
                selection.target_backend,
                resolve_output.clone(),
                ResolvedCheckSelection::from_command(selection.packages, cmd),
                user_log,
            )
        })
        .collect()
}

fn validate_selector_flags_before_split(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_backend: Option<TargetBackend>,
    user_log: &UserLog,
) -> anyhow::Result<()> {
    if cmd.patch_file.is_none() {
        return Ok(());
    }

    let selected =
        resolve_selected_packages(resolve_output, cmd, source_dir, target_backend, user_log)?;
    if cmd.patch_file.is_some() && selected.len() != 1 {
        anyhow::bail!("`--patch-file` requires the selector to resolve to a single package");
    }
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_check_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    user_log: &UserLog,
) -> anyhow::Result<(rr_build::BuildMeta, rr_build::BuildInput)> {
    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        target_dir,
        RunMode::Check,
    );

    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        user_log,
        &resolve_output,
    )?;
    let intent = if let Some(filter_path) = cmd.package_path.as_deref() {
        calc_user_intent_from_package_path(
            &resolve_output,
            source_dir,
            filter_path,
            planning_context.target_backend(),
            cmd.patch_file.as_deref(),
        )?
    } else {
        calc_user_intent(
            &resolve_output,
            &cmd.path,
            planning_context.target_backend(),
            cmd.patch_file.as_deref(),
            user_log,
        )?
    };
    rr_build::plan_resolved_build_from_intent(
        preconfig,
        &cli.unstable_feature,
        user_log,
        planning_context,
        intent,
        mooncake_bin_dir,
        resolve_output,
    )
}

#[allow(clippy::too_many_arguments)]
fn plan_check_rr_from_selection(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    target_backend: TargetBackend,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    selection: ResolvedCheckSelection,
    user_log: &UserLog,
) -> anyhow::Result<(rr_build::BuildMeta, rr_build::BuildInput)> {
    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        Some(target_backend),
        target_dir,
        RunMode::Check,
    );

    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        user_log,
        &resolve_output,
    )?;
    debug_assert_eq!(planning_context.target_backend(), target_backend);
    rr_build::plan_resolved_build_from_intent(
        preconfig,
        &cli.unstable_feature,
        user_log,
        planning_context,
        selection.into_user_intent()?,
        mooncake_bin_dir,
        resolve_output,
    )
}

pub(crate) fn resolve_check_target_selections(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    user_log: &UserLog,
) -> anyhow::Result<Vec<TargetPackageGroup>> {
    if let Some(target_backend) = selected_target_backend {
        let packages = resolve_selected_packages(
            resolve_output,
            cmd,
            source_dir,
            Some(target_backend),
            user_log,
        )?;
        return Ok(vec![TargetPackageGroup {
            target_backend,
            packages,
        }]);
    }

    let selected = resolve_selected_packages(resolve_output, cmd, source_dir, None, user_log)?;
    let selections = group_packages_by_preferred_backend(resolve_output, selected);

    let mut filtered = Vec::new();
    for selection in selections {
        let packages = filter_packages_for_backend(
            resolve_output,
            selection.packages,
            selection.target_backend,
        )?;
        if !packages.is_empty() {
            filtered.push(TargetPackageGroup {
                target_backend: selection.target_backend,
                packages,
            });
        }
    }

    Ok(filtered)
}

fn resolve_selected_packages(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_backend: Option<TargetBackend>,
    user_log: &UserLog,
) -> anyhow::Result<Vec<PackageId>> {
    if let Some(filter_path) = cmd.package_path.as_deref() {
        let (dir, _) = canonicalize_with_filename(&source_dir.join(filter_path))?;
        let pkg = filter_pkg_by_dir(resolve_output, &dir)?;
        if let Some(target_backend) = target_backend {
            ensure_package_supports_backend(resolve_output, pkg, target_backend)?;
        }
        return Ok(vec![pkg]);
    }

    if !cmd.path.is_empty() {
        if let Some(target_backend) = target_backend {
            return select_supported_packages(resolve_output, &cmd.path, target_backend, user_log);
        }
        return Ok(select_packages(&cmd.path, user_log, |dir| {
            filter_pkg_by_dir(resolve_output, dir)
        })?
        .into_iter()
        .map(|(_, pkg_id)| pkg_id)
        .collect());
    }

    Ok(rr_build::local_packages(resolve_output)
        .filter(|&pkg| {
            target_backend
                .is_none_or(|backend| package_supports_backend(resolve_output, pkg, backend))
        })
        .collect())
}

fn filter_packages_for_backend(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    packages: Vec<PackageId>,
    target_backend: TargetBackend,
) -> anyhow::Result<Vec<PackageId>> {
    let mut supported = Vec::new();
    let mut unsupported = Vec::new();

    for pkg in packages {
        if package_supports_backend(resolve_output, pkg, target_backend) {
            supported.push(pkg);
        } else {
            unsupported.push(pkg);
        }
    }

    if supported.is_empty() && !unsupported.is_empty() {
        if let [pkg] = unsupported.as_slice() {
            ensure_package_supports_backend(resolve_output, *pkg, target_backend)?;
        } else {
            ensure_packages_support_backend(
                resolve_output,
                unsupported.iter().copied(),
                target_backend,
            )?;
        }
    }

    Ok(supported)
}

fn calc_user_intent_from_package_path(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    source_dir: &Path,
    filter_path: &Path,
    target_backend: TargetBackend,
    patch_file: Option<&Path>,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let (dir, _) = canonicalize_with_filename(&source_dir.join(filter_path))?;
    let pkg = filter_pkg_by_dir(resolve_output, &dir)?;
    ensure_package_supports_backend(resolve_output, pkg, target_backend)?;
    let directive =
        rr_build::build_patch_directive_for_package(pkg, false, None, patch_file, false)?;
    Ok((vec![UserIntent::Check(pkg)], directive).into())
}

#[instrument(level = Level::DEBUG, skip_all)]
fn calc_user_intent(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    paths: &[PathBuf],
    target_backend: TargetBackend,
    patch_file: Option<&Path>,
    user_log: &UserLog,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    if !paths.is_empty() {
        let selected = select_supported_packages(resolve_output, paths, target_backend, user_log)?;
        let directive = build_directive_for_selected_packages(&selected, patch_file)?;
        Ok((
            selected.into_iter().map(UserIntent::Check).collect(),
            directive,
        )
            .into())
    } else {
        let intents: Vec<_> = rr_build::local_packages(resolve_output)
            .filter(|&pkg| package_supports_backend(resolve_output, pkg, target_backend))
            .map(UserIntent::Check)
            .collect();
        Ok(intents.into())
    }
}

fn build_directive_for_selected_packages(
    selected: &[moonbuild_rupes_recta::model::PackageId],
    patch_file: Option<&Path>,
) -> anyhow::Result<moonbuild_rupes_recta::build_plan::InputDirective> {
    if let [pkg] = selected {
        return rr_build::build_patch_directive_for_package(*pkg, false, None, patch_file, false);
    }

    if patch_file.is_some() {
        anyhow::bail!("`--patch-file` requires the selector to resolve to a single package");
    }
    Ok(Default::default())
}
