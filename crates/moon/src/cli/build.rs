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
use moonbuild::entry::N2RunStats;
use moonbuild_rupes_recta::intent::UserIntent;
use moonbuild_rupes_recta::model::PackageId;
use moonutil::common::FileLock;
use moonutil::common::RunMode;
use moonutil::common::TargetBackend;
use moonutil::common::lower_surface_targets;
use moonutil::dirs::PackageDirs;
use moonutil::mooncakes::ModuleId;
use moonutil::mooncakes::sync::AutoSyncFlags;
use std::path::{Path, PathBuf};
use tracing::{Level, instrument};

use crate::filter::{
    ensure_packages_support_backend, match_packages_by_name_rr, package_supports_backend,
    select_packages, select_supported_packages,
};
use crate::rr_build;
use crate::rr_build::BuildConfig;
use crate::rr_build::CalcUserIntentOutput;
use crate::rr_build::preconfig_compile;
use crate::user_diagnostics::UserDiagnostics;
use crate::watch::WatchOutput;
use crate::watch::prebuild_output::{PrebuildWatchPaths, rr_get_prebuild_watch_paths};
use crate::watch::watching;

use super::{BuildFlags, UniversalFlags};

enum BuildScope {
    Modules(Vec<ModuleId>),
    Packages(Vec<PackageId>),
}

struct BuildGroup {
    target_backend: TargetBackend,
    scope: BuildScope,
}

/// Build the current package
#[derive(Debug, clap::Parser)]
pub(crate) struct BuildSubcommand {
    /// Paths to the packages that should be built.
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
pub(crate) fn run_build(cli: &UniversalFlags, cmd: BuildSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
        mooncakes_dir,
        project_manifest_path,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    if cmd.build_flags.target.is_empty() {
        return run_build_internal(
            cli,
            &cmd,
            &source_dir,
            &target_dir,
            &mooncakes_dir,
            project_manifest_path.as_deref(),
            None,
        );
    }
    let surface_targets = cmd.build_flags.target.clone();
    let targets = lower_surface_targets(&surface_targets);

    let mut ret_value = 0;
    for t in targets {
        let x = run_build_internal(
            cli,
            &cmd,
            &source_dir,
            &target_dir,
            &mooncakes_dir,
            project_manifest_path.as_deref(),
            Some(t),
        )
        .context(format!("failed to run build for target {t:?}"))?;
        ret_value = ret_value.max(x);
    }
    Ok(ret_value)
}

#[instrument(skip_all)]
fn run_build_internal(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    mooncakes_dir: &Path,
    project_manifest_path: Option<&Path>,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    let f = |watch: bool| {
        run_build_rr(
            cli,
            cmd,
            source_dir,
            target_dir,
            mooncakes_dir,
            project_manifest_path,
            watch,
            selected_target_backend,
        )
    };

    if cmd.watch {
        watching(|| f(true), source_dir, source_dir, target_dir)
    } else {
        f(false).map(|output| if output.ok { 0 } else { 1 })
    }
}

/// Run the build routine in RR backend
///
/// - `_watch`: True if in watch mode, will output ignore paths for prebuild outputs
#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
fn run_build_rr(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    mooncakes_dir: &Path,
    project_manifest_path: Option<&Path>,
    _watch: bool,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<WatchOutput> {
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        !cmd.build_flags.std(),
        cmd.build_flags.enable_coverage,
    )
    .with_project_manifest_path(project_manifest_path);
    let resolve_output = moonbuild_rupes_recta::resolve(&resolve_cfg, source_dir, mooncakes_dir)?;
    if let Some(target_backend) = selected_target_backend {
        let (build_meta, build_graph) = plan_build_rr_from_resolved(
            cli,
            cmd,
            target_dir,
            Some(target_backend),
            None,
            None,
            resolve_output,
        )?;

        let prebuild_list = if _watch {
            rr_get_prebuild_watch_paths(&build_meta.resolve_output)
        } else {
            PrebuildWatchPaths {
                ignored_paths: Vec::new(),
                watched_paths: Vec::new(),
            }
        };

        let ok = if cli.dry_run {
            rr_build::print_dry_run(
                &build_graph,
                build_meta.artifacts.values(),
                source_dir,
                target_dir,
            );
            true
        } else {
            let _lock = FileLock::lock(target_dir)?;
            rr_build::generate_all_pkgs_json(target_dir, &build_meta, RunMode::Build)?;
            let result = rr_build::execute_build(
                &BuildConfig::from_flags(
                    &cmd.build_flags,
                    &cli.unstable_feature,
                    cli.verbose,
                    UserDiagnostics::from_flags(cli),
                ),
                build_graph,
                target_dir,
            )?;
            result.print_info(cli.quiet, "building")?;
            result.successful()
        };
        return Ok(WatchOutput {
            ok,
            additional_ignored_paths: prebuild_list.ignored_paths,
            additional_watched_paths: prebuild_list.watched_paths,
        });
    }

    let groups = resolve_build_groups(&resolve_output, cmd, UserDiagnostics::from_flags(cli))?;

    let mut prebuild_list = PrebuildWatchPaths {
        ignored_paths: Vec::new(),
        watched_paths: Vec::new(),
    };
    let mut planned = Vec::new();

    for group in groups {
        let module_scope = match &group.scope {
            BuildScope::Modules(modules) => Some(modules.as_slice()),
            BuildScope::Packages(_) => None,
        };
        let selected_packages = match &group.scope {
            BuildScope::Modules(_) => None,
            BuildScope::Packages(packages) => Some(packages.as_slice()),
        };
        let (build_meta, build_graph) = plan_build_rr_from_resolved(
            cli,
            cmd,
            target_dir,
            Some(group.target_backend),
            module_scope,
            selected_packages,
            resolve_output.clone(),
        )?;
        if _watch {
            let group_paths = rr_get_prebuild_watch_paths(&build_meta.resolve_output);
            prebuild_list
                .ignored_paths
                .extend(group_paths.ignored_paths);
            prebuild_list
                .watched_paths
                .extend(group_paths.watched_paths);
        }
        planned.push((build_meta, build_graph));
    }

    if planned.is_empty() {
        if !cli.dry_run {
            N2RunStats {
                n_tasks_executed: Some(0),
                n_errors: 0,
                n_warnings: 0,
            }
            .print_info(cli.quiet, "building")?;
        }
        return Ok(WatchOutput {
            ok: true,
            additional_ignored_paths: prebuild_list.ignored_paths,
            additional_watched_paths: prebuild_list.watched_paths,
        });
    }

    let ok = if cli.dry_run {
        for (build_meta, build_graph) in &planned {
            rr_build::print_dry_run(
                build_graph,
                build_meta.artifacts.values(),
                source_dir,
                target_dir,
            );
        }
        true
    } else {
        let _lock = FileLock::lock(target_dir)?;
        let build_config = BuildConfig::from_flags(
            &cmd.build_flags,
            &cli.unstable_feature,
            cli.verbose,
            UserDiagnostics::from_flags(cli),
        );
        let mut ok = true;
        for (build_meta, build_graph) in planned {
            rr_build::generate_all_pkgs_json(target_dir, &build_meta, RunMode::Build)?;
            let result = rr_build::execute_build(&build_config, build_graph, target_dir)?;
            result.print_info(cli.quiet, "building")?;
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

pub(crate) fn plan_build_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    target_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    module_scope: Option<&[ModuleId]>,
    selected_packages: Option<&[PackageId]>,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
) -> anyhow::Result<(rr_build::BuildMeta, rr_build::BuildInput)> {
    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        target_dir,
        RunMode::Build,
    );

    let output = UserDiagnostics::from_flags(cli);
    rr_build::plan_build_from_resolved(
        preconfig,
        &cli.unstable_feature,
        target_dir,
        output,
        Box::new(|resolved, target_backend| {
            calc_user_intent(
                &cmd.path,
                cmd.package.as_deref(),
                module_scope.unwrap_or(resolved.local_modules()),
                selected_packages,
                resolved,
                target_backend,
                output,
            )
        }),
        resolve_output,
    )
}

/// Generate user intent
/// If any packages are linkable, compile those; otherwise, compile everything
/// to core.
#[instrument(level = Level::DEBUG, skip_all)]
fn calc_user_intent(
    path_filters: &[PathBuf],
    package_filter: Option<&str>,
    module_scope: &[ModuleId],
    selected_packages: Option<&[PackageId]>,
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    target_backend: TargetBackend,
    output: UserDiagnostics,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    if let Some(selected_packages) = selected_packages {
        ensure_packages_support_backend(
            resolve_output,
            selected_packages.iter().copied(),
            target_backend,
        )?;
        Ok(selected_packages
            .iter()
            .copied()
            .map(UserIntent::Build)
            .collect::<Vec<_>>()
            .into())
    } else if !path_filters.is_empty() {
        let selected =
            select_supported_packages(resolve_output, path_filters, target_backend, output)?;
        Ok(selected
            .into_iter()
            .map(UserIntent::Build)
            .collect::<Vec<_>>()
            .into())
    } else if let Some(package_filter) = package_filter {
        let pkgs = match_packages_by_name_rr(resolve_output, module_scope, package_filter, output);
        ensure_packages_support_backend(resolve_output, pkgs.iter().copied(), target_backend)?;
        Ok(pkgs
            .into_iter()
            .map(UserIntent::Build)
            .collect::<Vec<_>>()
            .into())
    } else {
        let supported_packages = rr_build::local_packages_in_modules(resolve_output, module_scope)
            .filter(|&pkg_id| package_supports_backend(resolve_output, pkg_id, target_backend))
            .collect::<Vec<_>>();
        let linkable_pkgs = get_linkable_pkgs(
            resolve_output,
            target_backend,
            supported_packages.iter().copied(),
        );
        let intents: Vec<_> = if linkable_pkgs.is_empty() {
            supported_packages
                .into_iter()
                .filter(|&pkg_id| {
                    // Skip building stdlib packages because we should use prebuilt
                    // stdlib artifacts instead.
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

fn resolve_build_groups(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &BuildSubcommand,
    output: UserDiagnostics,
) -> anyhow::Result<Vec<BuildGroup>> {
    if !cmd.path.is_empty() {
        let selected = select_packages(&cmd.path, output, |dir| {
            crate::filter::filter_pkg_by_dir(resolve_output, dir)
        })?;
        return Ok(group_packages_by_default_target(
            resolve_output,
            selected.into_iter().map(|(_, pkg_id)| pkg_id),
        ));
    }

    if let Some(package_filter) = cmd.package.as_deref() {
        let pkgs = match_packages_by_name_rr(
            resolve_output,
            resolve_output.local_modules(),
            package_filter,
            output,
        );
        return Ok(group_packages_by_default_target(resolve_output, pkgs));
    }

    Ok(
        rr_build::group_modules_by_default_target(resolve_output, resolve_output.local_modules())
            .into_iter()
            .map(|(target_backend, modules)| BuildGroup {
                target_backend,
                scope: BuildScope::Modules(modules),
            })
            .collect(),
    )
}

fn group_packages_by_default_target(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    packages: impl IntoIterator<Item = PackageId>,
) -> Vec<BuildGroup> {
    let mut grouped = std::collections::BTreeMap::<TargetBackend, Vec<PackageId>>::new();
    for pkg_id in packages {
        let module_id = resolve_output.pkg_dirs.get_package(pkg_id).module;
        grouped
            .entry(rr_build::default_target_for_module(
                resolve_output,
                module_id,
            ))
            .or_default()
            .push(pkg_id);
    }
    grouped
        .into_iter()
        .map(|(target_backend, packages)| BuildGroup {
            target_backend,
            scope: BuildScope::Packages(packages),
        })
        .collect()
}
