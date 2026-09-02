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
use moonbuild_rupes_recta::intent::UserIntent;
use moonbuild_rupes_recta::model::PackageId;
use moonutil::build_options::RunMode;
use moonutil::cache::{CacheKind, resolve_cache_root};
use moonutil::cli_support::AutoSyncFlags;
use moonutil::command_output::CommandOutput;
use moonutil::locks::lock_directory;
use moonutil::project::PackageDirs;
use moonutil::target::TargetBackend;
use moonutil::target::lower_surface_targets;
use moonutil::user_log::UserLog;
use std::path::{Path, PathBuf};
use tracing::{Level, instrument};

use crate::filter::{
    TargetPackageGroup, ensure_packages_support_backend, filter_pkg_by_dir,
    group_packages_by_preferred_backend, match_packages_by_name_rr, package_supports_backend,
    select_packages, select_supported_packages,
};
use crate::rr_build;
use crate::rr_build::BuildConfig;
use crate::rr_build::CalcUserIntentOutput;
use crate::rr_build::preconfig_compile;
use crate::watch::prebuild_output::{PrebuildWatchPaths, rr_get_prebuild_watch_paths};
use crate::watch::{WatchOutput, watching};

use super::{BuildFlags, UniversalFlags};

#[derive(Debug, Clone)]
struct ResolvedBuildSelection {
    packages: Vec<PackageId>,
}

impl ResolvedBuildSelection {
    fn into_user_intent(self) -> CalcUserIntentOutput {
        self.packages
            .into_iter()
            .map(UserIntent::Build)
            .collect::<Vec<_>>()
            .into()
    }
}

/// Build the current package
#[derive(Debug, clap::Parser, Clone)]
pub(crate) struct BuildSubcommand {
    /// Paths to the packages that should be built, or one standalone `.mbtx` file.
    #[clap(name = "PATH", conflicts_with("package"))]
    pub path: Vec<PathBuf>,

    #[clap(flatten)]
    pub build_flags: BuildFlags,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Monitor the file system and automatically build artifacts
    #[clap(long, short)]
    pub watch: bool,

    // package name (username/hello/lib)
    #[clap(long, hide = true)]
    pub package: Option<String>,
}

#[instrument(skip_all)]
pub(crate) fn run_build(
    cli: &UniversalFlags,
    cmd: BuildSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let targets = lower_surface_targets(&cmd.build_flags.target);
    if let Some(path) = super::standalone_mbtx_path(&cmd.path, "moon build")? {
        anyhow::ensure!(
            !cmd.watch,
            "standalone `.mbtx` `moon build` does not support `--watch`"
        );
        let single_file = cli.source_tgt_dir.single_file_package_dirs(path)?;
        return run_build_for_single_file_rr(
            cli,
            &cmd,
            &single_file.file_path,
            &single_file.package_dirs,
            &targets,
            output,
        );
    }

    let dirs = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(output.user_log())?
        .package_dirs()?;

    if cmd.build_flags.target.is_empty() {
        return run_build_internal(cli, &cmd, &dirs, None, output);
    }
    // Watch reruns must synchronize and resolve fresh project state each time.
    if cmd.watch {
        let mut ret_value = 0;
        for t in targets {
            let x = run_build_internal(cli, &cmd, &dirs, Some(t), output)
                .context(format!("failed to run build for target {t:?}"))?;
            ret_value = ret_value.max(x);
        }
        return Ok(ret_value);
    }

    let resolve_output = sync_and_resolve_build_project(cli, &cmd, &dirs, output.user_log())?;
    let _lock;
    if !cli.dry_run {
        _lock = lock_directory(&dirs.target_dir, output.user_log())?;
    }
    let result =
        run_build_rr_from_resolved(cli, &cmd, &dirs, false, &targets, resolve_output, output)
            .with_context(|| match targets.as_slice() {
                [target] => format!("failed to run build for target {target:?}"),
                _ => format!("failed to run build for targets {targets:?}"),
            })?;
    Ok(if result.ok { 0 } else { 1 })
}

/// Resolve one standalone `.mbtx` input and build it for every selected Target Backend.
#[allow(clippy::too_many_arguments)]
fn run_build_for_single_file_rr(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    single_file_path: &Path,
    dirs: &PackageDirs,
    selected_target_backends: &[TargetBackend],
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let user_log = output.user_log();
    let PackageDirs {
        source_dir,
        target_dir,
        mooncake_bin_dir,
        ..
    } = dirs;
    std::fs::create_dir_all(target_dir).context("failed to create target directory")?;

    let resolve_config = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        !cmd.build_flags.std(),
        cmd.build_flags.enable_coverage,
        cli.workspace_env.clone(),
    )
    .with_dependency_source_cache(
        resolve_cache_root(CacheKind::DependencySources)
            .context("Failed to resolve the module dependency graph")?,
    );
    let (resolved, backend) = moonbuild_rupes_recta::resolve::resolve_single_file_project(
        &resolve_config,
        dirs,
        single_file_path,
        true,
        user_log,
    )?;
    let target_backends = if selected_target_backends.is_empty() {
        vec![cmd.build_flags.resolve_single_target_backend()?.or(backend)]
    } else {
        selected_target_backends.iter().copied().map(Some).collect()
    };

    let _lock;
    if !cli.dry_run {
        _lock = lock_directory(target_dir, user_log)?;
    }

    let package = rr_build::local_packages(&resolved)
        .next()
        .context("single-file project must resolve exactly one local package")?;
    let mut planned_runs = Vec::with_capacity(target_backends.len());
    for target_backend in target_backends {
        let preconfig = preconfig_compile(
            &cmd.auto_sync_flags,
            cli,
            &cmd.build_flags,
            target_backend,
            target_dir,
            RunMode::Build,
        );
        let planning_context = rr_build::prepare_resolved_build(
            &preconfig,
            &cli.unstable_feature,
            target_dir,
            user_log,
            &resolved,
        )?;
        planned_runs.push(rr_build::plan_resolved_standalone_build_from_intent(
            preconfig,
            &cli.unstable_feature,
            user_log,
            planning_context,
            vec![UserIntent::Build(package)].into(),
            package,
            mooncake_bin_dir,
            resolved.clone(),
        )?);
    }

    let ok = if cli.dry_run {
        output.write_result(|writer| {
            let (build_metas, build_inputs): (Vec<_>, Vec<_>) = planned_runs.into_iter().unzip();
            let build_input = rr_build::compose_standalone_build_inputs(build_inputs)
                .map_err(std::io::Error::other)?;
            rr_build::write_standalone_dry_run(
                writer,
                &build_input,
                build_metas.iter().flat_map(|meta| meta.artifacts.values()),
                source_dir,
                target_dir,
            )?;
            Ok::<_, std::io::Error>(())
        })?;
        true
    } else {
        for (build_meta, _) in &planned_runs {
            rr_build::generate_all_pkgs_json(build_meta)?;
        }
        let build_input = rr_build::compose_standalone_build_inputs(
            planned_runs
                .into_iter()
                .map(|(_, build_input)| build_input)
                .collect(),
        )?;
        let config = BuildConfig::from_flags(&cmd.build_flags, &cli.unstable_feature, cli.verbose);
        let result =
            rr_build::execute_standalone_build(&config, build_input, target_dir, user_log)?;
        result.print_info(cli.quiet, "building")?;
        result.successful()
    };
    Ok(if ok { 0 } else { 1 })
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
fn run_build_internal(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    dirs: &PackageDirs,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let f = |watch: bool| run_build_rr(cli, cmd, dirs, watch, selected_target_backend, output);

    if cmd.watch {
        watching(|| f(true), &dirs.source_dir, &dirs.target_dir)
    } else {
        f(false).map(|output| if output.ok { 0 } else { 1 })
    }
}

fn sync_and_resolve_build_project(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    dirs: &PackageDirs,
    user_log: &UserLog,
) -> anyhow::Result<moonbuild_rupes_recta::ResolveOutput> {
    let resolve_config = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        !cmd.build_flags.std(),
        cmd.build_flags.enable_coverage,
        cli.workspace_env.clone(),
    );
    rr_build::sync_and_resolve_project(&resolve_config, dirs, user_log)
}

/// Run the build routine in RR backend
///
/// - `watch`: True if in watch mode, will output ignore paths for prebuild outputs
#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
fn run_build_rr(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    dirs: &PackageDirs,
    watch: bool,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
) -> anyhow::Result<WatchOutput> {
    let resolve_output = sync_and_resolve_build_project(cli, cmd, dirs, output.user_log())?;
    let _lock;
    if !cli.dry_run {
        _lock = lock_directory(&dirs.target_dir, output.user_log())?;
    }
    run_build_rr_from_resolved(
        cli,
        cmd,
        dirs,
        watch,
        selected_target_backend.as_slice(),
        resolve_output,
        output,
    )
}

/// Plans and executes a build from resolved project data.
///
/// The caller must hold the target-directory lock for a non-dry-run build.
#[allow(clippy::too_many_arguments)]
fn run_build_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    dirs: &PackageDirs,
    watch: bool,
    selected_target_backends: &[TargetBackend],
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    output: &CommandOutput,
) -> anyhow::Result<WatchOutput> {
    let user_log = output.user_log();
    let PackageDirs {
        source_dir,
        target_dir,
        mooncake_bin_dir,
        ..
    } = dirs;
    let prebuild_list = if watch {
        rr_get_prebuild_watch_paths(&resolve_output)
    } else {
        PrebuildWatchPaths {
            ignored_paths: Vec::new(),
            watched_paths: Vec::new(),
        }
    };
    let planned_runs = if selected_target_backends.is_empty() {
        plan_build_rr_from_resolved_all(
            cli,
            cmd,
            source_dir,
            target_dir,
            mooncake_bin_dir,
            None,
            resolve_output,
            user_log,
        )?
    } else {
        selected_target_backends
            .iter()
            .copied()
            .map(|target| {
                plan_build_rr_from_resolved_all(
                    cli,
                    cmd,
                    source_dir,
                    target_dir,
                    mooncake_bin_dir,
                    Some(target),
                    resolve_output.clone(),
                    user_log,
                )
            })
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect()
    };

    if planned_runs.is_empty() {
        return Ok(WatchOutput {
            ok: true,
            additional_ignored_paths: prebuild_list.ignored_paths,
            additional_watched_paths: prebuild_list.watched_paths,
        });
    }

    let ok = if cli.dry_run {
        output.write_result(|writer| {
            let (build_metas, build_inputs): (Vec<_>, Vec<_>) = planned_runs.into_iter().unzip();
            let build_input =
                rr_build::compose_build_inputs(build_inputs).map_err(std::io::Error::other)?;
            rr_build::write_dry_run(
                writer,
                &build_input,
                build_metas.iter().flat_map(|meta| meta.artifacts.values()),
                source_dir,
                target_dir,
            )?;
            Ok::<_, std::io::Error>(())
        })?;
        true
    } else {
        let cfg = BuildConfig::from_flags(&cmd.build_flags, &cli.unstable_feature, cli.verbose);
        for (build_meta, _) in &planned_runs {
            rr_build::generate_all_pkgs_json(build_meta)?;
        }
        let build_inputs = planned_runs.into_iter().map(|(_, input)| input).collect();
        let build_input = rr_build::compose_build_inputs(build_inputs)?;
        let result = rr_build::execute_build(&cfg, build_input, target_dir, user_log)?;
        result.print_info(cli.quiet, "building")?;
        result.successful()
    };
    Ok(WatchOutput {
        ok,
        additional_ignored_paths: prebuild_list.ignored_paths,
        additional_watched_paths: prebuild_list.watched_paths,
    })
}

pub(crate) fn plan_build_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
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
        RunMode::Build,
    );

    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        user_log,
        &resolve_output,
    )?;
    let intent = calc_user_intent(
        &cmd.path,
        cmd.package.as_deref(),
        &resolve_output,
        planning_context.target_backend(),
        user_log,
    )?;
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
fn plan_build_rr_from_resolved_with_scope(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    target_backend: TargetBackend,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    scoped_packages: Vec<PackageId>,
    user_log: &UserLog,
) -> anyhow::Result<(rr_build::BuildMeta, rr_build::BuildInput)> {
    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        Some(target_backend),
        target_dir,
        RunMode::Build,
    );

    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        user_log,
        &resolve_output,
    )?;
    debug_assert_eq!(planning_context.target_backend(), target_backend);
    let intent = calc_user_intent_from_scoped_packages(
        &resolve_output,
        &scoped_packages,
        planning_context.target_backend(),
    )?;
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
fn plan_build_rr_from_selection(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    target_backend: TargetBackend,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    selection: ResolvedBuildSelection,
    user_log: &UserLog,
) -> anyhow::Result<(rr_build::BuildMeta, rr_build::BuildInput)> {
    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        Some(target_backend),
        target_dir,
        RunMode::Build,
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
        selection.into_user_intent(),
        mooncake_bin_dir,
        resolve_output,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_build_rr_from_resolved_all(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    _source_dir: &Path,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    user_log: &UserLog,
) -> anyhow::Result<Vec<(rr_build::BuildMeta, rr_build::BuildInput)>> {
    if let Some(target_backend) = selected_target_backend {
        if has_explicit_build_selector(cmd) {
            let packages = resolve_selected_build_packages(
                &resolve_output,
                cmd,
                Some(target_backend),
                user_log,
            )?;
            if packages.is_empty() {
                return Ok(Vec::new());
            }

            return plan_build_rr_from_selection(
                cli,
                cmd,
                target_dir,
                mooncake_bin_dir,
                target_backend,
                resolve_output,
                ResolvedBuildSelection { packages },
                user_log,
            )
            .map(|plan| vec![plan]);
        }

        return plan_build_rr_from_resolved(
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

    let selections = resolve_build_target_selections(&resolve_output, cmd, None, user_log)?;

    if has_explicit_build_selector(cmd) {
        return selections
            .into_iter()
            .map(|selection| {
                // Raw user selector paths/package names have already been
                // resolved into PackageIds. RR will use those identities plus
                // the bin-dependency launcher directory captured by the
                // command adapter.
                plan_build_rr_from_selection(
                    cli,
                    cmd,
                    target_dir,
                    mooncake_bin_dir,
                    selection.target_backend,
                    resolve_output.clone(),
                    ResolvedBuildSelection {
                        packages: selection.packages,
                    },
                    user_log,
                )
            })
            .collect();
    }

    if selections.is_empty() {
        return plan_build_rr_from_resolved(
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
            plan_build_rr_from_resolved_with_scope(
                cli,
                cmd,
                target_dir,
                mooncake_bin_dir,
                selection.target_backend,
                resolve_output.clone(),
                selection.packages,
                user_log,
            )
        })
        .collect()
}

fn has_explicit_build_selector(cmd: &BuildSubcommand) -> bool {
    !cmd.path.is_empty() || cmd.package.is_some()
}

fn resolve_build_target_selections(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &BuildSubcommand,
    selected_target_backend: Option<TargetBackend>,
    user_log: &UserLog,
) -> anyhow::Result<Vec<TargetPackageGroup>> {
    if let Some(target_backend) = selected_target_backend {
        let packages =
            resolve_selected_build_packages(resolve_output, cmd, Some(target_backend), user_log)?;
        if packages.is_empty() {
            return Ok(Vec::new());
        }
        return Ok(vec![TargetPackageGroup {
            target_backend,
            packages,
        }]);
    }

    let selected = resolve_selected_build_packages(resolve_output, cmd, None, user_log)?;
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

fn resolve_selected_build_packages(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &BuildSubcommand,
    target_backend: Option<TargetBackend>,
    user_log: &UserLog,
) -> anyhow::Result<Vec<PackageId>> {
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

    if let Some(package_filter) = cmd.package.as_deref() {
        let pkgs = match_packages_by_name_rr(
            resolve_output,
            resolve_output.local_modules(),
            package_filter,
            user_log,
        );
        if let Some(target_backend) = target_backend {
            ensure_packages_support_backend(resolve_output, pkgs.iter().copied(), target_backend)?;
        }
        return Ok(pkgs);
    }

    Ok(rr_build::local_packages(resolve_output)
        .filter(|&pkg_id| {
            target_backend
                .is_none_or(|backend| package_supports_backend(resolve_output, pkg_id, backend))
        })
        .collect())
}

/// Generate user intent
/// If any packages are linkable, compile those; otherwise, compile everything
/// to core.
#[instrument(level = Level::DEBUG, skip_all)]
fn calc_user_intent(
    path_filters: &[PathBuf],
    package_filter: Option<&str>,
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    target_backend: TargetBackend,
    user_log: &UserLog,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    if !path_filters.is_empty() {
        let selected =
            select_supported_packages(resolve_output, path_filters, target_backend, user_log)?;
        Ok(selected
            .into_iter()
            .map(UserIntent::Build)
            .collect::<Vec<_>>()
            .into())
    } else if let Some(package_filter) = package_filter {
        let pkgs = match_packages_by_name_rr(
            resolve_output,
            resolve_output.local_modules(),
            package_filter,
            user_log,
        );
        ensure_packages_support_backend(resolve_output, pkgs.iter().copied(), target_backend)?;
        Ok(pkgs
            .into_iter()
            .map(UserIntent::Build)
            .collect::<Vec<_>>()
            .into())
    } else {
        calc_user_intent_from_scoped_packages(
            resolve_output,
            &rr_build::local_packages(resolve_output)
                .filter(|&pkg_id| package_supports_backend(resolve_output, pkg_id, target_backend))
                .collect::<Vec<_>>(),
            target_backend,
        )
    }
}

fn calc_user_intent_from_scoped_packages(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    supported_packages: &[PackageId],
    target_backend: TargetBackend,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let linkable_pkgs = get_linkable_pkgs(
        resolve_output,
        target_backend,
        supported_packages.iter().copied(),
    );
    let intents: Vec<_> = if linkable_pkgs.is_empty() {
        supported_packages
            .iter()
            .copied()
            .filter(|&pkg_id| {
                let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
                !pkg.is_stdlib
            })
            .map(UserIntent::Build)
            .collect()
    } else {
        linkable_pkgs.into_iter().map(UserIntent::Build).collect()
    };
    Ok(intents.into())
}

fn get_linkable_pkgs(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    target_backend: TargetBackend,
    packages: impl Iterator<Item = PackageId>,
) -> Vec<PackageId> {
    let mut linkable_pkgs = vec![];
    for pkg_id in packages {
        let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
        if pkg.raw.force_link
            || pkg
                .raw
                .link
                .as_ref()
                .is_some_and(|link| link.need_link(target_backend))
            || pkg.raw.is_main
        {
            linkable_pkgs.push(pkg_id)
        }
    }
    linkable_pkgs
}
