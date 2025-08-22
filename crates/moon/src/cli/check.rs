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
use moonbuild::dry_run;
use moonbuild::watch::watching;
use moonbuild::watcher_is_running;
use moonbuild::{entry, MOON_PID_NAME};
use moonbuild_rupes_recta::model::{BuildPlanNode, TargetKind};
use mooncake::pkg::sync::auto_sync;
use moonutil::cli::UniversalFlags;
use moonutil::common::{lower_surface_targets, CheckOpt};
use moonutil::common::{parse_front_matter_config, WATCH_MODE_DIR};
use moonutil::common::{FileLock, TargetBackend};
use moonutil::common::{MoonbuildOpt, PrePostBuild};
use moonutil::common::{MooncOpt, OutputFormat, RunMode};
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use n2::trace;
use std::path::{Path, PathBuf};

use crate::cli::get_module_for_single_file;
use crate::rr_build::{self, preconfig_compile};

use super::pre_build::scan_with_x_build;
use super::{get_compiler_flags, BuildFlags};

/// Check the current package, but don't build object files
#[derive(Debug, clap::Parser, Clone)]
pub struct CheckSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    /// Output in json format
    #[clap(long)]
    pub output_json: bool,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Monitor the file system and automatically check files
    #[clap(long, short)]
    pub watch: bool,

    /// The package(and it's deps) to check
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
    #[clap(conflicts_with = "watch")]
    pub single_file: Option<PathBuf>,
}

pub fn run_check(cli: &UniversalFlags, cmd: &CheckSubcommand) -> anyhow::Result<i32> {
    let (source_dir, mut target_dir) = if let Some(ref single_file_path) = cmd.single_file {
        let single_file_path = &dunce::canonicalize(single_file_path).unwrap();
        let source_dir = single_file_path.parent().unwrap().to_path_buf();
        let target_dir = source_dir.join("target");
        (source_dir, target_dir)
    } else {
        let dir = cli.source_tgt_dir.try_into_package_dirs()?;
        (dir.source_dir, dir.target_dir)
    };

    // make a dedicated directory for the watch mode so that we don't block(MOON_LOCK) the normal no-watch mode(automatically trigger by ide in background)
    if cmd.watch {
        target_dir = target_dir.join(WATCH_MODE_DIR);
        std::fs::create_dir_all(&target_dir).context(format!(
            "Failed to create target directory: '{}'",
            target_dir.display()
        ))?;
    };

    if cmd.build_flags.target.is_none() {
        return run_check_internal(cli, cmd, &source_dir, &target_dir);
    }

    let surface_targets = cmd.build_flags.target.clone().unwrap();
    let targets = lower_surface_targets(&surface_targets);

    let mut ret_value = 0;
    for t in targets {
        let mut cmd = (*cmd).clone();
        cmd.build_flags.target_backend = Some(t);
        let x = run_check_internal(cli, &cmd, &source_dir, &target_dir)
            .context(format!("failed to run check for target {t:?}"))?;
        ret_value = ret_value.max(x);
    }
    Ok(ret_value)
}

fn run_check_internal(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
) -> anyhow::Result<i32> {
    if cmd.single_file.is_some() {
        run_check_for_single_file(cli, cmd)
    } else {
        run_check_normal_internal(cli, cmd, source_dir, target_dir)
    }
}

fn run_check_for_single_file(cli: &UniversalFlags, cmd: &CheckSubcommand) -> anyhow::Result<i32> {
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
            package_path: None,
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
        output_json: false,
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
        render: !cmd.build_flags.no_render,
        single_file: true,
    };
    let module =
        get_module_for_single_file(single_file_path, &moonc_opt, &moonbuild_opt, mbt_md_header)?;

    if cli.dry_run {
        return dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt);
    }

    entry::run_check(&moonc_opt, &moonbuild_opt, &module)
}

fn run_check_normal_internal(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
) -> anyhow::Result<i32> {
    if cli.unstable_feature.rupes_recta {
        let preconfig = preconfig_compile(&cmd.auto_sync_flags, cli, &cmd.build_flags, target_dir);
        let (_build_meta, build_graph) = rr_build::plan_build(
            preconfig,
            &cli.unstable_feature,
            source_dir,
            target_dir,
            Box::new(calc_user_intent),
        )?;

        if cli.dry_run {
            rr_build::print_dry_run(&build_graph, &_build_meta.artifacts, source_dir, target_dir);
            Ok(0)
        } else {
            let result = rr_build::execute_build(build_graph, target_dir)?;
            result.print_info();
            Ok(result.return_code_for_success())
        }
    } else {
        run_check_normal_internal_legacy(cli, cmd, source_dir, target_dir)
    }
}

fn run_check_normal_internal_legacy(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
) -> anyhow::Result<i32> {
    // Run moon install before build
    let (resolved_env, dir_sync_result) = auto_sync(
        source_dir,
        &cmd.auto_sync_flags,
        &RegistryConfig::load(),
        cli.quiet,
    )?;

    let raw_target_dir = target_dir;
    let run_mode = RunMode::Check;
    let mut moonc_opt = get_compiler_flags(source_dir, &cmd.build_flags)?;
    moonc_opt.build_opt.deny_warn = cmd.build_flags.deny_warn;
    let target_dir = mk_arch_mode_dir(source_dir, target_dir, &moonc_opt, run_mode)?;
    let _lock = FileLock::lock(&target_dir)?;

    // TODO: remove this once LLVM backend is well supported
    if moonc_opt.build_opt.target_backend == TargetBackend::LLVM {
        eprintln!("{}: LLVM backend is experimental and only supported on bleeding moonbit toolchain for now", "Warning".yellow());
    }

    let sort_input = cmd.build_flags.sort_input;

    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.to_path_buf(),
        raw_target_dir: raw_target_dir.to_path_buf(),
        target_dir: target_dir.clone(),
        sort_input,
        run_mode,
        quiet: cli.quiet,
        verbose: cli.verbose,
        output_json: cmd.output_json,
        build_graph: cli.build_graph,
        check_opt: Some(CheckOpt {
            package_path: cmd.package_path.clone(),
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

    if let Some(CheckOpt {
        package_path: Some(pkg_path),
        patch_file: pp,
        no_mi: nm,
        ..
    }) = moonbuild_opt.check_opt.as_ref()
    {
        let pkg_by_path = module.get_package_by_path_mut(&moonbuild_opt.source_dir.join(pkg_path));
        if let Some(specified_pkg) = pkg_by_path {
            specified_pkg.no_mi = *nm;
            specified_pkg.patch_file = pp.clone();
        }
    };

    if cli.dry_run {
        return dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt);
    }

    if cli.trace {
        trace::open("trace.json").context("failed to open `trace.json`")?;
    }

    let watch_mode = cmd.watch;

    let res = if watch_mode {
        let reg_cfg = RegistryConfig::load();
        watching(
            &moonc_opt,
            &moonbuild_opt,
            &reg_cfg,
            &module,
            raw_target_dir,
        )
    } else {
        let pid_path = target_dir.join(MOON_PID_NAME);
        let running = watcher_is_running(&pid_path);

        if let Ok(true) = running {
            let output_path = target_dir.join("check.output");
            let output = std::fs::read_to_string(&output_path)
                .context(format!("failed to open `{}`", output_path.display()))?;
            if !output.trim().is_empty() {
                println!("{}", output.trim());
            }
            Ok(if output.is_empty() { 0 } else { 1 })
        } else {
            entry::run_check(&moonc_opt, &moonbuild_opt, &module)
        }
    };

    if cli.trace {
        trace::close();
    }

    res
}

/// Generate user intent
///
/// Check all packages in the current module.
fn calc_user_intent(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    main_modules: &[moonutil::mooncakes::ModuleId],
) -> Result<Vec<BuildPlanNode>, anyhow::Error> {
    let &[main_module_id] = main_modules else {
        panic!("No multiple main modules are supported");
    };

    let packages = resolve_output
        .pkg_dirs
        .packages_for_module(main_module_id)
        .ok_or_else(|| anyhow::anyhow!("Cannot find the local module!"))?;

    let nodes = packages
        .iter()
        .flat_map(|(_, &pkg_id)| {
            [
                TargetKind::Source,
                TargetKind::WhiteboxTest,
                TargetKind::BlackboxTest,
            ]
            .map(|x| BuildPlanNode::check(pkg_id.build_target(x)))
        })
        .collect();

    Ok(nodes)
}
