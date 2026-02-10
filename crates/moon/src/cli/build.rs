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
use anyhow::anyhow;
use moonbuild_rupes_recta::intent::UserIntent;
use moonbuild_rupes_recta::model::PackageId;
use moonutil::common::FileLock;
use moonutil::common::RunMode;
use moonutil::common::TargetBackend;
use moonutil::common::lower_surface_targets;
use moonutil::dirs::PackageDirs;
use moonutil::mooncakes::sync::AutoSyncFlags;
use std::path::Path;
use std::path::PathBuf;
use tracing::{Level, instrument};

use crate::filter::match_packages_by_name_rr;
use crate::filter::{canonicalize_with_filename, filter_pkg_by_dir};
use crate::rr_build;
use crate::rr_build::BuildConfig;
use crate::rr_build::CalcUserIntentOutput;
use crate::rr_build::preconfig_compile;
use crate::watch::WatchOutput;
use crate::watch::prebuild_output::rr_get_prebuild_ignored_paths;
use crate::watch::watching;

use super::{BuildFlags, UniversalFlags};

/// Build the current package
#[derive(Debug, clap::Parser, Clone)]
pub struct BuildSubcommand {
    /// The path to the package that should be built.
    #[clap(name = "PATH", conflicts_with("package"))]
    pub path: Option<PathBuf>,

    #[clap(flatten)]
    pub build_flags: BuildFlags,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Monitor the file system and automatically build artifacts
    #[clap(long, short)]
    pub watch: bool,

    #[clap(long, hide = true)]
    pub install_path: Option<PathBuf>,

    #[clap(long, hide = true)]
    pub show_artifacts: bool,

    // package name (username/hello/lib)
    #[clap(long, hide = true)]
    pub package: Option<String>,

    // when package is specified, specify the alias of the binary package artifact to install
    #[clap(long, hide = true, requires("package"))]
    pub bin_alias: Option<String>,
}

#[instrument(skip_all)]
pub fn run_build(cli: &UniversalFlags, cmd: BuildSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    if cmd.build_flags.target.is_empty() {
        return run_build_internal(cli, &cmd, &source_dir, &target_dir, None);
    }
    let surface_targets = cmd.build_flags.target.clone();
    let targets = lower_surface_targets(&surface_targets);

    let mut ret_value = 0;
    for t in targets {
        let x = run_build_internal(cli, &cmd, &source_dir, &target_dir, Some(t))
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
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    let f = |watch: bool| {
        run_build_rr(
            cli,
            cmd,
            source_dir,
            target_dir,
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
fn run_build_rr(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    _watch: bool,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<WatchOutput> {
    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        target_dir,
        RunMode::Build,
    );
    let (build_meta, build_graph) = rr_build::plan_build(
        preconfig,
        &cli.unstable_feature,
        source_dir,
        target_dir,
        Box::new(|resolve_output, target_backend| {
            calc_user_intent(
                cmd.path.as_deref(),
                cmd.package.as_deref(),
                resolve_output,
                target_backend,
            )
        }),
    )?;

    // Prepare for `watch` mode
    let prebuild_list = if _watch {
        rr_get_prebuild_ignored_paths(&build_meta.resolve_output)
    } else {
        Vec::new()
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
        // Generate all_pkgs.json for indirect dependency resolution
        rr_build::generate_all_pkgs_json(target_dir, &build_meta, RunMode::Build)?;

        let result = rr_build::execute_build(
            &BuildConfig::from_flags(&cmd.build_flags, &cli.unstable_feature, cli.verbose),
            build_graph,
            target_dir,
        )?;
        result.print_info(cli.quiet, "building")?;

        result.successful()
    };
    Ok(WatchOutput {
        ok,
        additional_ignored_paths: prebuild_list,
    })
}

/// Generate user intent
/// If any packages are linkable, compile those; otherwise, compile everything
/// to core.
#[instrument(level = Level::DEBUG, skip_all)]
fn calc_user_intent(
    path_filter: Option<&Path>,
    package_filter: Option<&str>,
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    target_backend: TargetBackend,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    if let Some(path) = path_filter {
        let (dir, _) = canonicalize_with_filename(path)?;
        let pkg = filter_pkg_by_dir(resolve_output, &dir)?;
        Ok(vec![UserIntent::Build(pkg)].into())
    } else if let Some(package_filter) = package_filter {
        let pkg = match_packages_by_name_rr(
            resolve_output,
            resolve_output.local_modules(),
            package_filter,
        );
        Ok(pkg
            .into_iter()
            .map(UserIntent::Build)
            .collect::<Vec<_>>()
            .into())
    } else {
        let &[main_module_id] = resolve_output.local_modules() else {
            panic!("No multiple main modules are supported");
        };

        let packages = resolve_output
            .pkg_dirs
            .packages_for_module(main_module_id)
            .ok_or_else(|| anyhow!("Cannot find the local module!"))?;
        let linkable_pkgs =
            get_linkable_pkgs(resolve_output, target_backend, packages.values().cloned())?;
        let intents: Vec<_> = if linkable_pkgs.is_empty() {
            packages
                .iter()
                .filter(|&(_, &pkg_id)| {
                    // Skip building stdlib packages
                    // because we should use prebuilt stdlib artifacts instead
                    let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
                    !pkg.is_stdlib
                })
                .map(|(_, &pkg_id)| UserIntent::Build(pkg_id))
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
) -> anyhow::Result<Vec<PackageId>> {
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
    Ok(linkable_pkgs)
}
