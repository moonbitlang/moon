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
use moonbuild::{dry_run, entry};
use moonbuild_rupes_recta::intent::UserIntent;
use mooncake::pkg::sync::auto_sync;
use moonutil::cli::UniversalFlags;
use moonutil::common::{CheckOpt, lower_surface_targets};
use moonutil::common::{FileLock, TargetBackend};
use moonutil::common::{MoonbuildOpt, PrePostBuild};
use moonutil::common::{MooncOpt, OutputFormat, RunMode};
use moonutil::common::{WATCH_MODE_DIR, parse_front_matter_config};
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::mooncakes::RegistryConfig;
use moonutil::mooncakes::sync::AutoSyncFlags;
use std::path::{Path, PathBuf};
use tracing::{Level, instrument};

use crate::cli::get_module_for_single_file;
use crate::filter::{canonicalize_with_filename, filter_pkg_by_dir};
use crate::rr_build::{self, BuildConfig, CalcUserIntentOutput, preconfig_compile};
use crate::watch::prebuild_output::{
    legacy_get_prebuild_ignored_paths, rr_get_prebuild_ignored_paths,
};
use crate::watch::{WatchOutput, watching};

use super::pre_build::scan_with_x_build;
use super::{BuildFlags, get_compiler_flags};

/// Check the current package, but don't build object files
#[derive(Debug, clap::Parser, Clone)]
pub struct CheckSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Monitor the file system and automatically check files
    #[clap(long, short)]
    pub watch: bool,

    /// The package(and it's deps) to check
    //
    // TODO: Unify the `-p` flag to specifying package name, see #1139
    #[clap(long, short)]
    pub package_path: Option<PathBuf>,

    /// The patch file to check, Only valid when checking specified package.
    #[clap(long, requires = "package_path")]
    pub patch_file: Option<PathBuf>,

    /// Whether to skip the mi generation, Only valid when checking specified package.
    #[clap(long, requires = "package_path")]
    pub no_mi: bool,

    /// Whether to explain the error code with details.
    #[clap(long)]
    pub explain: bool,

    /// Check single file (.mbt or .mbt.md)
    #[clap(conflicts_with = "watch", name = "PATH")]
    pub path: Option<PathBuf>,
}

#[instrument(skip_all)]
pub fn run_check(cli: &UniversalFlags, cmd: &CheckSubcommand) -> anyhow::Result<i32> {
    // Check if we're running within a project
    let (source_dir, target_dir, single_file) = match cli.source_tgt_dir.try_into_package_dirs() {
        Ok(dirs) => (dirs.source_dir, dirs.target_dir, false),
        Err(e @ moonutil::dirs::PackageDirsError::NotInProject(_)) => {
            // Now we're talking about real single-file scenario.
            if let Some(path) = cmd.path.as_deref() {
                let single_file_path = &dunce::canonicalize(path).unwrap();
                let source_dir = single_file_path.parent().unwrap().to_path_buf();
                let target_dir = source_dir.join("target");
                (source_dir, target_dir, true)
            } else {
                return Err(e.into());
            }
        }
        Err(e) => {
            return Err(e.into());
        }
    };

    if cmd.build_flags.target.is_none() {
        return run_check_internal(cli, cmd, &source_dir, &target_dir, single_file);
    }

    let surface_targets = cmd.build_flags.target.clone().unwrap();
    let targets = lower_surface_targets(&surface_targets);

    let mut ret_value = 0;
    for t in targets {
        let mut cmd = (*cmd).clone();
        cmd.build_flags.target_backend = Some(t);
        let x = run_check_internal(cli, &cmd, &source_dir, &target_dir, single_file)
            .context(format!("failed to run check for target {t:?}"))?;
        ret_value = ret_value.max(x);
    }
    Ok(ret_value)
}

#[instrument(skip_all)]
fn run_check_internal(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    single_file: bool,
) -> anyhow::Result<i32> {
    if single_file {
        run_check_for_single_file(cli, cmd)
    } else {
        run_check_normal_internal(cli, cmd, source_dir, target_dir)
    }
}

fn run_check_for_single_file(cli: &UniversalFlags, cmd: &CheckSubcommand) -> anyhow::Result<i32> {
    if cli.unstable_feature.rupes_recta {
        run_check_for_single_file_rr(cli, cmd)
    } else {
        run_check_for_single_file_legacy(cli, cmd)
    }
}

fn run_check_for_single_file_rr(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
) -> anyhow::Result<i32> {
    let single_file_path = &dunce::canonicalize(cmd.path.as_ref().unwrap()).unwrap();
    let source_dir = single_file_path.parent().unwrap().to_path_buf();
    let raw_target_dir = source_dir.join("target");
    std::fs::create_dir_all(&raw_target_dir).context("failed to create target directory")?;

    let mut cmd = cmd.clone();

    cmd.build_flags.populate_target_backend_from_list()?;

    // Manually synthesize and resolve single file project
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        RegistryConfig::load(),
        false,
        cmd.build_flags.enable_coverage,
    );
    let (resolved, backend) = moonbuild_rupes_recta::resolve::resolve_single_file_project(
        &resolve_cfg,
        single_file_path,
        false,
    )?;

    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags.clone().with_default_target_backend(backend),
        &raw_target_dir,
        moonutil::cond_expr::OptLevel::Release,
        RunMode::Check,
    );

    // The rest is similar to normal check flow
    let (build_meta, build_graph) = rr_build::plan_build_from_resolved(
        preconfig,
        &cli.unstable_feature,
        &raw_target_dir,
        Box::new(get_user_intents_single_file),
        resolved,
    )?;

    if cli.dry_run {
        rr_build::print_dry_run(
            &build_graph,
            build_meta.artifacts.values(),
            &source_dir,
            &raw_target_dir,
        );
        return Ok(0);
    }

    let _lock = FileLock::lock(&raw_target_dir)?;

    // Generate all_pkgs.json for indirect dependency resolution
    rr_build::generate_all_pkgs_json(&raw_target_dir, &build_meta, RunMode::Check)?;
    let filename = single_file_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(String::from);
    rr_build::generate_metadata(
        &source_dir,
        &raw_target_dir,
        &build_meta,
        RunMode::Check,
        filename.as_deref(),
    )?;

    let mut cfg = BuildConfig::from_flags(&cmd.build_flags, &cli.unstable_feature, cli.verbose);
    cfg.patch_file = cmd.patch_file.clone();
    cfg.explain_errors |= cmd.explain;

    let result = rr_build::execute_build(&cfg, build_graph, &raw_target_dir)?;
    result.print_info(cli.quiet, "checking")?;

    Ok(if result.successful() { 0 } else { 1 })
}

fn get_user_intents_single_file(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    _backend: TargetBackend,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let m_packages = resolve_output
        .pkg_dirs
        .packages_for_module(resolve_output.local_modules()[0])
        .expect("Local module must exist");
    let pkg = *m_packages
        .iter()
        .next()
        .expect("Only one package should be resolved for single file package")
        .1;

    Ok(vec![UserIntent::Check(pkg)].into())
}

#[instrument(level = Level::DEBUG, skip_all)]
fn run_check_for_single_file_legacy(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
) -> anyhow::Result<i32> {
    let single_file_path = &dunce::canonicalize(cmd.path.as_ref().unwrap()).unwrap();
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

    let release_flag = !cmd.build_flags.debug;

    let target_dir = raw_target_dir
        .join(target_backend.to_dir_name())
        .join(if release_flag { "release" } else { "debug" })
        .join(RunMode::Check.to_dir_name());

    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.clone(),
        target_dir: target_dir.clone(),
        raw_target_dir: raw_target_dir.clone(),
        test_opt: None,
        check_opt: Some(CheckOpt {
            package_name_filter: None, // Single file check has no package filter
            patch_file: None,
            no_mi: cmd.no_mi,
            explain: cmd.explain,
        }),
        build_opt: None,
        sort_input: cmd.build_flags.sort_input,
        run_mode: RunMode::Check,
        quiet: cli.quiet,
        verbose: cli.verbose,
        no_parallelize: false,
        build_graph: cli.build_graph,
        fmt_opt: None,
        args: vec![],
        no_render_output: cmd.build_flags.output_style().needs_no_render(),
        parallelism: cmd.build_flags.jobs,
        use_tcc_run: false,
        dynamic_stub_libs: None,
        render_no_loc: cmd.build_flags.render_no_loc,
    };
    let moonc_opt = MooncOpt {
        build_opt: moonutil::common::BuildPackageFlags {
            debug_flag: !release_flag,
            strip_flag: false,
            source_map: false,
            enable_coverage: false,
            deny_warn: false,
            target_backend,
            warn_list: cmd.build_flags.warn_list.clone(),
            alert_list: cmd.build_flags.alert_list.clone(),
            enable_value_tracing: cmd.build_flags.enable_value_tracing,
        },
        link_opt: moonutil::common::LinkCoreFlags {
            debug_flag: !release_flag,
            source_map: !release_flag,
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
        json_diagnostics: cmd.build_flags.output_style().needs_moonc_json(),
        single_file: true,
    };
    let module =
        get_module_for_single_file(single_file_path, &moonc_opt, &moonbuild_opt, mbt_md_header)?;

    if cli.dry_run {
        return dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt);
    }

    entry::run_check(&moonc_opt, &moonbuild_opt, &module)
}

#[instrument(skip_all)]
fn run_check_normal_internal(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
) -> anyhow::Result<i32> {
    let run_once = |watch: bool, target_dir: &Path| -> anyhow::Result<WatchOutput> {
        if cli.unstable_feature.rupes_recta {
            run_check_normal_internal_rr(cli, cmd, source_dir, target_dir, watch)
        } else {
            run_check_normal_internal_legacy(cli, cmd, source_dir, target_dir, watch)
        }
    };
    if cmd.watch {
        // For checks, the actual target dir is a subdir of the original target
        let watch_target = target_dir.join(WATCH_MODE_DIR);
        std::fs::create_dir_all(&watch_target).with_context(|| {
            format!(
                "Failed to create target directory: '{}'",
                watch_target.display()
            )
        })?;
        watching(
            || run_once(true, &watch_target),
            source_dir,
            source_dir,
            target_dir,
        )
    } else {
        run_once(false, target_dir).map(|output| if output.ok { 0 } else { 1 })
    }
}

#[instrument(skip_all)]
fn run_check_normal_internal_rr(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    _watch: bool,
) -> anyhow::Result<WatchOutput> {
    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        target_dir,
        moonutil::cond_expr::OptLevel::Release,
        RunMode::Check,
    );

    let (build_meta, build_graph) = rr_build::plan_build(
        preconfig,
        &cli.unstable_feature,
        source_dir,
        target_dir,
        Box::new(|r, _tb| {
            calc_user_intent(
                r,
                source_dir,
                cmd.package_path.as_deref(),
                cmd.path.as_deref(),
                cmd.no_mi,
                cmd.patch_file.as_deref(),
            )
        }),
    )
    .context("Failed to calculate build plan")?;

    let prebuild_list = if _watch {
        rr_get_prebuild_ignored_paths(&build_meta.resolve_output)
    } else {
        Vec::new()
    };

    if cli.dry_run {
        rr_build::print_dry_run(
            &build_graph,
            build_meta.artifacts.values(),
            source_dir,
            target_dir,
        );
        Ok(WatchOutput {
            ok: true,
            additional_ignored_paths: prebuild_list,
        })
    } else {
        let _lock = FileLock::lock(target_dir)?;
        // Generate all_pkgs.json for indirect dependency resolution
        rr_build::generate_all_pkgs_json(target_dir, &build_meta, RunMode::Check)?;
        // Generate metadata for IDE
        rr_build::generate_metadata(source_dir, target_dir, &build_meta, RunMode::Check, None)?;

        let mut cfg = BuildConfig::from_flags(&cmd.build_flags, &cli.unstable_feature, cli.verbose);
        cfg.patch_file = cmd.patch_file.clone();
        cfg.explain_errors |= cmd.explain;
        let result = rr_build::execute_build(&cfg, build_graph, target_dir)?;
        result.print_info(cli.quiet, "checking")?;
        Ok(WatchOutput {
            ok: result.successful(),
            additional_ignored_paths: prebuild_list,
        })
    }
}

#[instrument(skip_all)]
fn run_check_normal_internal_legacy(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    _watch: bool,
) -> anyhow::Result<WatchOutput> {
    // Run moon install before build
    let (resolved_env, dir_sync_result) = auto_sync(
        source_dir,
        &cmd.auto_sync_flags,
        &RegistryConfig::load(),
        cli.quiet,
        true, // Legacy don't need std injection
    )?;

    let raw_target_dir = target_dir;
    let run_mode = RunMode::Check;
    let mut moonc_opt = get_compiler_flags(source_dir, &cmd.build_flags)?;
    moonc_opt.build_opt.deny_warn = cmd.build_flags.deny_warn;
    let target_dir = mk_arch_mode_dir(source_dir, target_dir, &moonc_opt, run_mode)?;
    let _lock = FileLock::lock(&target_dir)?;

    // TODO: remove this once LLVM backend is well supported
    if moonc_opt.build_opt.target_backend == TargetBackend::LLVM {
        eprintln!(
            "{}: LLVM backend is experimental and only supported on bleeding moonbit toolchain for now",
            "Warning".yellow()
        );
    }

    let sort_input = cmd.build_flags.sort_input;

    let mut moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.to_path_buf(),
        raw_target_dir: raw_target_dir.to_path_buf(),
        target_dir: target_dir.clone(),
        sort_input,
        run_mode,
        quiet: cli.quiet,
        verbose: cli.verbose,
        no_render_output: cmd.build_flags.output_style().needs_no_render(),
        build_graph: cli.build_graph,
        check_opt: Some(CheckOpt {
            package_name_filter: None,
            // ^ Set below. Strange to put it here, but didn't bother changing now.
            patch_file: cmd.patch_file.clone(),
            no_mi: cmd.no_mi,
            explain: cmd.explain,
        }),
        test_opt: None,
        build_opt: None,
        fmt_opt: None,
        args: vec![],
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

    {
        let nm = cmd.no_mi;
        let pp = &cmd.patch_file;

        // Filter packages using the two flags
        let filtered_package_name = if let Some(pkg_path) = &cmd.package_path {
            // This path is relative to source_dir
            let path = dunce::canonicalize(source_dir.join(pkg_path))
                .context("Cannot canonicalize package name")?;
            let pkg_name = module
                .get_package_by_path(&path)
                .with_context(|| {
                    format!(
                        "Cannot find package at given package path '{}'",
                        pkg_path.display()
                    )
                })?
                .full_name();
            Some(pkg_name)
        } else if let Some(path) = &cmd.path {
            let (canonical_path, _) = canonicalize_with_filename(path).with_context(|| {
                format!("Cannot canonicalize provided path '{}'", path.display())
            })?;
            let pkg_name = module
                .get_package_by_path(&canonical_path)
                .with_context(|| {
                    format!(
                        "Cannot find package at path '{}' (resolved to '{}')",
                        path.display(),
                        canonical_path.display(),
                    )
                })?
                .full_name();
            Some(pkg_name)
        } else {
            None
        };

        if let Some(pkg_name) = filtered_package_name {
            let pkg_by_name = module.get_package_by_name_mut_safe(&pkg_name);
            if let Some(specified_pkg) = pkg_by_name {
                specified_pkg.no_mi = nm;
                specified_pkg.patch_file = pp.clone();
            } else {
                panic!(
                    "Package '{}' not found in module, but it was queried from path earlier. This is a bug.",
                    pkg_name,
                );
            }

            // Set the package name filter
            moonbuild_opt
                .check_opt
                .as_mut()
                .unwrap()
                .package_name_filter = Some(pkg_name);
        }
    };

    let prebuild_list = if _watch {
        legacy_get_prebuild_ignored_paths(&module)
    } else {
        Vec::new()
    };

    if cli.dry_run {
        let exit_code = dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt)?;
        return Ok(WatchOutput {
            ok: exit_code == 0,
            additional_ignored_paths: prebuild_list,
        });
    }

    let res = entry::run_check(&moonc_opt, &moonbuild_opt, &module);

    let exit_code = res?;
    Ok(WatchOutput {
        ok: exit_code == 0,
        additional_ignored_paths: prebuild_list,
    })
}

/// Generate user intent of checking all packages in the current module.
///
/// Two paths are supported:
/// - `package_path`: The legacy `-p` flag, specifying the path from the source
///   dir to the package to check.
/// - `path`: The new positional argument, specifying a relative path from the
///   working directory to a package directory.
/// Only one of them can be specified at a time.
#[instrument(level = Level::DEBUG, skip_all)]
fn calc_user_intent(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    source_dir: &Path,
    package_path: Option<&Path>,
    path: Option<&Path>,
    no_mi: bool,
    patch_file: Option<&Path>,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let &[_main_module_id] = resolve_output.local_modules() else {
        panic!("No multiple main modules are supported");
    };

    if package_path.is_some() && path.is_some() {
        anyhow::bail!(
            "Only one of `-p/--package-path` and positional `PATH` can be specified at a time"
        );
    }

    if let Some(filter_path) = package_path {
        let (dir, _) = canonicalize_with_filename(&source_dir.join(filter_path))?;
        let pkg = filter_pkg_by_dir(resolve_output, &dir)?;

        // Apply --no-mi and --patch-file to specific packages
        let directive =
            rr_build::build_patch_directive_for_package(pkg, no_mi, None, patch_file, false)?;

        Ok((vec![UserIntent::Check(pkg)], directive).into())
    } else if let Some(check_path) = path {
        let (dir, _) = canonicalize_with_filename(check_path)?;
        let pkg = filter_pkg_by_dir(resolve_output, &dir)?;

        // Apply --no-mi and --patch-file to specific packages
        let directive =
            rr_build::build_patch_directive_for_package(pkg, no_mi, None, patch_file, false)?;

        Ok((vec![UserIntent::Check(pkg)], directive).into())
    } else {
        let intents: Vec<_> = resolve_output
            .pkg_dirs
            .all_packages()
            .map(|(id, _)| UserIntent::Check(id))
            .collect();
        Ok(intents.into())
    }
}
