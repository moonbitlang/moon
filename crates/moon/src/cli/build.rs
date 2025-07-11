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

use anyhow::anyhow;
use anyhow::Context;
use colored::Colorize;
use moonbuild::dry_run;
use moonbuild::entry;
use moonbuild::watch::watching;
use moonbuild_rupes_recta::compile::UserIntent;
use moonbuild_rupes_recta::model::BuildTarget;
use moonbuild_rupes_recta::model::TargetKind;
use moonbuild_rupes_recta::resolve::ResolveConfig;
use moonbuild_rupes_recta::CompileContext;
use mooncake::pkg::sync::auto_sync;
use moonutil::common::lower_surface_targets;
use moonutil::common::BuildOpt;
use moonutil::common::FileLock;
use moonutil::common::MoonbuildOpt;
use moonutil::common::PrePostBuild;
use moonutil::common::RunMode;
use moonutil::common::TargetBackend;
use moonutil::cond_expr::OptLevel;
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::dirs::PackageDirs;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use n2::trace;
use std::path::Path;
use std::path::PathBuf;

use super::pre_build::scan_with_x_build;
use super::{BuildFlags, UniversalFlags};

/// Build the current package
#[derive(Debug, clap::Parser, Clone)]
pub struct BuildSubcommand {
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

fn run_build_internal(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    source_dir: &Path,
    target_dir: &Path,
) -> anyhow::Result<i32> {
    if cli.unstable_feature.rupes_recta {
        run_build_internal_rupes_recta(cli, cmd, source_dir, target_dir)
    } else {
        run_build_internal_legacy(cli, cmd, source_dir, target_dir)
    }
}

fn run_build_internal_legacy(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
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
    let run_mode = RunMode::Build;
    let mut moonc_opt = super::get_compiler_flags(source_dir, &cmd.build_flags)?;
    moonc_opt.build_opt.deny_warn = cmd.build_flags.deny_warn;
    let target_dir = mk_arch_mode_dir(source_dir, target_dir, &moonc_opt, run_mode)?;
    let _lock = FileLock::lock(&target_dir)?;
    let sort_input = cmd.build_flags.sort_input;

    // TODO: remove this once LLVM backend is well supported
    if moonc_opt.build_opt.target_backend == TargetBackend::LLVM {
        eprintln!("{}: LLVM backend is experimental and only supported on bleeding moonbit toolchain for now", "Warning".yellow());
    }

    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.to_path_buf(),
        raw_target_dir: raw_target_dir.to_path_buf(),
        target_dir,
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
        output_json: false,
        no_parallelize: false,
        parallelism: cmd.build_flags.jobs,
        use_tcc_run: false,
        dynamic_stub_libs: None,
    };

    let mut module = scan_with_x_build(
        false,
        &moonc_opt,
        &moonbuild_opt,
        &resolved_env,
        &dir_sync_result,
        &PrePostBuild::PreBuild,
    )?;

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

    if cli.dry_run {
        return dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt);
    }

    let trace_flag = cli.trace;
    if trace_flag {
        trace::open("trace.json").context("failed to open `trace.json`")?;
    }

    let res = if cmd.watch {
        let reg_cfg = RegistryConfig::load();
        watching(
            &moonc_opt,
            &moonbuild_opt,
            &reg_cfg,
            &module,
            raw_target_dir,
        )
    } else {
        entry::run_build(&moonc_opt, &moonbuild_opt, &module)
    };

    if trace_flag {
        trace::close();
    }
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
    res
}

fn run_build_internal_rupes_recta(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    source_dir: &Path,
    target_dir: &Path,
) -> anyhow::Result<i32> {
    let cfg = ResolveConfig::new_with_load_defaults(cmd.auto_sync_flags.frozen);
    let resolve_output = moonbuild_rupes_recta::resolve(&cfg, source_dir)?;

    // A couple of debug things:
    if cli.unstable_feature.rr_export_module_graph {
        moonbuild_rupes_recta::util::print_resolved_env_dot(
            &resolve_output.module_rel,
            &mut std::fs::File::create(target_dir.join("module_graph.dot"))?,
        )?;
    }
    if cli.unstable_feature.rr_export_package_graph {
        moonbuild_rupes_recta::util::print_dep_relationship_dot(
            &resolve_output.pkg_rel,
            &resolve_output.pkg_dirs,
            &mut std::fs::File::create(target_dir.join("package_graph.dot"))?,
        )?;
    }

    assert_eq!(
        resolve_output.local_modules().len(),
        1,
        "There should be exactly one main local module, got {:?}",
        resolve_output.local_modules()
    );
    let main_module_id = resolve_output.local_modules()[0];
    let main_module = resolve_output.module_rel.module_info(main_module_id);

    // Preferred backend
    let preferred_backend = main_module.preferred_target;

    let intent = calc_user_intent(&resolve_output, main_module_id)?;

    let cx = CompileContext {
        resolve_output: &resolve_output,
        target_dir: target_dir.to_owned(),
        target_backend: cmd
            .build_flags
            .target_backend
            .or(preferred_backend)
            .unwrap_or_default(),
        opt_level: if cmd.build_flags.release {
            OptLevel::Release
        } else {
            OptLevel::Debug
        },
        debug_symbols: !cmd.build_flags.release || cmd.build_flags.debug,
    };
    let graph = moonbuild_rupes_recta::compile(&cx, &[intent])?;

    // Generate n2 state
    // FIXME: This is extremely verbose and barebones, only for testing purpose
    let mut graph = graph.build_graph;
    let mut hashes = n2::graph::Hashes::default();
    let n2_db = n2::db::open(
        &target_dir.join("moon.rupes-recta.db"),
        &mut graph,
        &mut hashes,
    )?;
    let mut prog_console = n2::progress::DumbConsoleProgress::new(false, None);
    let mut work = n2::work::Work::new(
        graph,
        hashes,
        n2_db,
        &n2::work::Options {
            failures_left: Some(1),
            parallelism: 1,
            explain: false,
            adopt: false,
            dirty_on_output: true,
        },
        &mut prog_console,
        n2::smallmap::SmallMap::default(),
    );
    work.want_every_file(None)?;
    let res = work.run()?;
    if let Some(n) = res {
        println!("{n} tasks executed");
        Ok(0)
    } else {
        println!("Build failed");
        Ok(1)
    }
}

/// Generate user intent
/// If any packages are linkable, compile those; otherwise, compile everything
/// to core.
fn calc_user_intent(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    main_module_id: moonutil::mooncakes::ModuleId,
) -> Result<UserIntent, anyhow::Error> {
    let packages = resolve_output
        .pkg_dirs
        .packages_for_module(main_module_id)
        .ok_or_else(|| anyhow!("Cannot find the local module!"))?;
    let mut linkable_pkgs = vec![];
    for &pkg_id in packages.values() {
        let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
        if pkg.raw.force_link || pkg.raw.link.is_some() || pkg.raw.is_main {
            linkable_pkgs.push(pkg_id)
        }
    }
    let intent = if linkable_pkgs.is_empty() {
        UserIntent::BuildCore(
            packages
                .iter()
                .map(|(_, &pkg_id)| BuildTarget {
                    package: pkg_id,
                    kind: TargetKind::Source,
                })
                .collect(),
        )
    } else {
        UserIntent::BuildExecutable(
            linkable_pkgs
                .into_iter()
                .map(|pkg_id| BuildTarget {
                    package: pkg_id,
                    kind: TargetKind::Source,
                })
                .collect(),
        )
    };
    Ok(intent)
}
