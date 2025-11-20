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
use colored::Colorize;
use moonbuild::dry_run;
use moonbuild::entry;
use moonbuild_rupes_recta::intent::UserIntent;
use moonbuild_rupes_recta::model::PackageId;
use mooncake::pkg::sync::auto_sync;
use moonutil::common::BuildOpt;
use moonutil::common::FileLock;
use moonutil::common::MoonbuildOpt;
use moonutil::common::PrePostBuild;
use moonutil::common::RunMode;
use moonutil::common::TargetBackend;
use moonutil::common::lower_surface_targets;
use moonutil::cond_expr::OptLevel;
use moonutil::dirs::PackageDirs;
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::mooncakes::RegistryConfig;
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
use crate::watch::prebuild_output::legacy_get_prebuild_ignored_paths;
use crate::watch::prebuild_output::rr_get_prebuild_ignored_paths;
use crate::watch::watching;

use super::pre_build::scan_with_x_build;
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
pub fn run_build(cli: &UniversalFlags, cmd: &BuildSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    if cmd.build_flags.target.is_none() {
        return run_build_internal(cli, cmd, &source_dir, &target_dir);
    }
    let surface_targets = cmd.build_flags.target.clone().unwrap();
    let targets = lower_surface_targets(&surface_targets);

    let mut ret_value = 0;
    for t in targets {
        let mut cmd = (*cmd).clone();
        cmd.build_flags.target_backend = Some(t);
        let x = run_build_internal(cli, &cmd, &source_dir, &target_dir)
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
) -> anyhow::Result<i32> {
    let f = |watch: bool| {
        if cli.unstable_feature.rupes_recta {
            run_build_rr(cli, cmd, source_dir, target_dir, watch)
        } else {
            run_build_legacy(cli, cmd, source_dir, target_dir, watch)
        }
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
) -> anyhow::Result<WatchOutput> {
    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        target_dir,
        OptLevel::Release,
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

        let result = rr_build::execute_build(
            &BuildConfig::from_flags(&cmd.build_flags, &cli.unstable_feature),
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

#[instrument(skip_all)]
fn run_build_legacy(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    _watch: bool,
) -> anyhow::Result<WatchOutput> {
    let path_filter_dir = cmd
        .path
        .as_ref()
        .map(|path| canonicalize_with_filename(path).map(|(dir, _)| dir))
        .transpose()?;

    // Run moon install before build
    let (resolved_env, dir_sync_result) = auto_sync(
        source_dir,
        &cmd.auto_sync_flags,
        &RegistryConfig::load(),
        cli.quiet,
        true, // Legacy don't need std injection
    )?;

    let raw_target_dir = target_dir;
    let run_mode = RunMode::Build;
    let mut moonc_opt = super::get_compiler_flags(source_dir, &cmd.build_flags)?;
    moonc_opt.build_opt.deny_warn = cmd.build_flags.deny_warn;
    let target_dir = mk_arch_mode_dir(source_dir, target_dir, &moonc_opt, run_mode)?;
    let _lock = FileLock::lock(&target_dir)?;
    let sort_input = cmd.build_flags.sort_input;

    // TODO: remove this once LLVM backend is well supported
    if moonc_opt.build_opt.target_backend == TargetBackend::LLVM {
        eprintln!(
            "{}: LLVM backend is experimental and only supported on bleeding moonbit toolchain for now",
            "Warning".yellow()
        );
    }

    let mut moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.to_path_buf(),
        raw_target_dir: raw_target_dir.to_path_buf(),
        target_dir: target_dir.to_path_buf(),
        sort_input,
        run_mode,
        quiet: cli.quiet,
        verbose: cli.verbose,
        build_graph: cli.build_graph,
        test_opt: None,
        check_opt: None,
        build_opt: Some(BuildOpt {
            install_path: cmd.install_path.clone(),
            filter_package: cmd.package.clone(),
        }),
        fmt_opt: None,
        args: vec![],
        no_render_output: cmd.build_flags.output_style().needs_no_render(),
        no_parallelize: false,
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

    if let Some(dir) = path_filter_dir.as_ref() {
        let pkg = module.get_package_by_path(dir).ok_or_else(|| {
            anyhow!(
                "Cannot find package to build based on input path `{}`",
                dir.display()
            )
        })?;
        if let Some(build_opt) = moonbuild_opt.build_opt.as_mut() {
            build_opt.filter_package = Some(pkg.full_name().to_owned());
        }
    }

    if let Some(bin_alias) = cmd.bin_alias.clone() {
        let pkg = module.get_package_by_name_mut_safe(cmd.package.as_ref().unwrap());
        match pkg {
            Some(pkg) => {
                pkg.bin_name = Some(bin_alias);
            }
            _ => anyhow::bail!(format!(
                "package `{}` not found",
                cmd.package.as_ref().unwrap()
            )),
        }
    }

    moonutil::common::set_native_backend_link_flags(
        run_mode,
        moonc_opt.build_opt.target_backend,
        &mut module,
    )?;

    let prebuild_list = if _watch {
        legacy_get_prebuild_ignored_paths(&module)
    } else {
        Vec::new()
    };

    if cli.dry_run {
        let ret = dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt)?;
        return Ok(WatchOutput {
            ok: ret == 0,
            additional_ignored_paths: prebuild_list,
        });
    }

    let res = entry::run_build(&moonc_opt, &moonbuild_opt, &module);

    if let (Ok(_), true) = (res.as_ref(), cmd.show_artifacts) {
        // can't use HashMap because the order of the packages is not guaranteed
        // can't use IndexMap because moonc cannot handled ordered map
        let mut artifacts = Vec::new();
        for pkg in module
            .get_topo_pkgs()?
            .iter()
            .filter(|pkg| !pkg.is_third_party)
        {
            let mi = pkg.artifact.with_extension("mi");
            let core = pkg.artifact.with_extension("core");
            artifacts.push((pkg.full_name(), mi, core));
        }
        println!("{}", serde_json::to_string(&artifacts).unwrap());
    }
    let ok = res? == 0;
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
