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

use std::path::Path;

use anyhow::{Context, bail};
use moonbuild::dry_run;
use moonbuild::entry;
use moonbuild_rupes_recta::build_plan::InputDirective;
use moonbuild_rupes_recta::intent::UserIntent;
use mooncake::pkg::sync::auto_sync;
use moonutil::common::BUILD_DIR;
use moonutil::common::FileLock;
use moonutil::common::MOONBITLANG_CORE;
use moonutil::common::PrePostBuild;
use moonutil::common::RunMode;
use moonutil::common::SurfaceTarget;
use moonutil::common::TargetBackend;
use moonutil::common::TestArtifacts;
use moonutil::common::is_moon_pkg_exist;
use moonutil::common::lower_surface_targets;
use moonutil::common::{MoonbuildOpt, OutputFormat};
use moonutil::cond_expr::OptLevel;
use moonutil::cond_expr::OptLevel::Release;
use moonutil::dirs::PackageDirs;
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::moon_dir::MOON_DIRS;
use moonutil::mooncakes::RegistryConfig;
use moonutil::mooncakes::sync::AutoSyncFlags;
use n2::trace;
use tracing::{Level, instrument};

use crate::rr_build;
use crate::rr_build::preconfig_compile;
use crate::rr_build::{BuildConfig, CalcUserIntentOutput};
use crate::run::CommandGuard;
use crate::run::default_rt;

use super::pre_build::scan_with_x_build;
use super::{BuildFlags, UniversalFlags};

/// Run a main package
#[derive(Debug, clap::Parser, Clone)]
pub struct RunSubcommand {
    /// The package or .mbt file to run
    pub package_or_mbt_file: String,

    #[clap(flatten)]
    pub build_flags: BuildFlags,

    /// The arguments provided to the program to be run
    pub args: Vec<String>,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Only build, do not run the code
    #[clap(long)]
    pub build_only: bool,
}

#[instrument(skip_all)]
pub fn run_run(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    // Falling back to legacy to support running standalone single mbt file This
    // is currently how the `moon test` handles single file as well. We should
    // have a RR solution later.
    match cli.source_tgt_dir.try_into_package_dirs() {
        Ok(_) => {
            if cmd.package_or_mbt_file.ends_with(".mbt") {
                let moon_pkg_json_exist = std::env::current_dir()?
                    .join(&cmd.package_or_mbt_file)
                    .parent()
                    .is_some_and(is_moon_pkg_exist);
                if !moon_pkg_json_exist {
                    if cli.unstable_feature.rupes_recta {
                        return run_single_file_rr(cli, cmd);
                    } else {
                        return run_single_mbt_file(cli, cmd);
                    }
                }
            }
            // moon should report an error later if the source_dir doesn't
            // contain moon.pkg.json
        }
        Err(e @ moonutil::dirs::PackageDirsError::NotInProject(_)) => {
            if cmd.package_or_mbt_file.ends_with(".mbt") {
                if cli.unstable_feature.rupes_recta {
                    return run_single_file_rr(cli, cmd);
                } else {
                    return run_single_mbt_file(cli, cmd);
                }
            } else {
                return Err(e.into());
            }
        }
        Err(e) => return Err(e.into()),
    }

    if let Some(surface_targets) = &cmd.build_flags.target {
        for st in surface_targets.iter() {
            if *st == SurfaceTarget::All {
                anyhow::bail!("`--target all` is not supported for `run`");
            }
        }

        if surface_targets.len() > 1 {
            anyhow::bail!("`--target` only supports one target for `run`")
        }

        let targets = lower_surface_targets(surface_targets);
        for t in targets {
            let mut cmd = cmd.clone();
            cmd.build_flags.target_backend = Some(t);
            run_run_internal(cli, cmd)?;
        }
        Ok(0)
    } else {
        run_run_internal(cli, cmd)
    }
}

#[instrument(level = Level::DEBUG, skip_all)]
fn run_single_mbt_file(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    let current_dir = std::env::current_dir()?;
    let mbt_file_path = dunce::canonicalize(current_dir.join(cmd.package_or_mbt_file))?;
    let mbt_file_parent_path = mbt_file_path.parent().unwrap();

    if !mbt_file_path.is_file() {
        bail!("{} is not exist or not a file", mbt_file_path.display());
    }

    let file_name = mbt_file_path.file_stem().unwrap().to_str().unwrap();

    let target_backend = lower_surface_targets(&cmd.build_flags.target.unwrap_or_default())
        .first()
        .map_or(TargetBackend::default(), |it| *it);
    let core_bundle_path = moonutil::moon_dir::core_bundle(target_backend);

    let output_artifact_path = mbt_file_parent_path.join(BUILD_DIR);

    let output_core_path = &(output_artifact_path
        .join(format!("{file_name}.core"))
        .display()
        .to_string());

    let output_wasm_or_js_path =
        output_artifact_path.join(format!("{}.{}", file_name, target_backend.to_artifact()));

    let pkg_name = "moon/run/single";
    let mut build_package_command = vec![
        "build-package".to_string(),
        mbt_file_path.display().to_string(),
        "-o".to_string(),
        output_core_path.to_string(),
        "-std-path".to_string(),
        core_bundle_path.to_str().unwrap().to_string(),
        "-is-main".to_string(),
        "-pkg".to_string(),
        pkg_name.to_string(),
        "-g".to_string(),
        "-O0".to_string(),
        "-source-map".to_string(),
        "-target".to_string(),
        target_backend.to_flag().to_string(),
    ];
    if cmd.build_flags.enable_value_tracing {
        build_package_command.push("-enable-value-tracing".to_string());
    }
    let mut link_core_command = vec![
        "link-core".to_string(),
        // dirty workaround for now
        moonutil::moon_dir::core_core(target_backend)[0].clone(),
        moonutil::moon_dir::core_core(target_backend)[1].clone(),
        output_artifact_path
            .join(format!("{file_name}.core"))
            .display()
            .to_string(),
        "-o".to_string(),
        output_wasm_or_js_path.display().to_string(),
        "-pkg-sources".to_string(),
        format!("{}:{}", pkg_name, mbt_file_parent_path.display()),
        "-pkg-sources".to_string(),
        format!(
            "{}:{}",
            MOONBITLANG_CORE,
            moonutil::moon_dir::core().display()
        ),
        "-g".to_string(),
        "-O0".to_string(),
        "-source-map".to_string(),
        "-target".to_string(),
        target_backend.to_flag().to_string(),
    ];

    let cc_default = moonutil::compiler_flags::CC::default();
    if cc_default.is_msvc() && target_backend == TargetBackend::LLVM {
        link_core_command.extend([
            "-llvm-target".to_string(),
            "x86_64-pc-windows-msvc".to_string(),
        ]);
    }

    // runtime.c on Windows cannot be built with tcc
    // it's expensive to use cl.exe to build one first
    // and then use tcc to load it
    let use_tcc_run =
        !cfg!(windows) && target_backend == TargetBackend::Native && !cmd.build_flags.release;

    let moon_lib_path = &MOON_DIRS.moon_lib_path;

    let compile_exe_command = if use_tcc_run {
        let tcc_run_command = vec![
            MOON_DIRS.internal_tcc_path.display().to_string(),
            format!("-I{}", MOON_DIRS.moon_include_path.display()),
            format!("-L{}", MOON_DIRS.moon_lib_path.display()),
            moon_lib_path.join("runtime.c").display().to_string(),
            "-lm".to_string(),
            "-DMOONBIT_NATIVE_NO_SYS_HEADER".to_string(),
            "-run".to_string(),
            output_wasm_or_js_path.display().to_string(),
        ];
        Some(tcc_run_command)
    } else if target_backend == TargetBackend::Native || target_backend == TargetBackend::LLVM {
        let cc_cmd = moonutil::compiler_flags::make_cc_command::<&'static str>(
            cc_default,
            None,
            moonutil::compiler_flags::CCConfigBuilder::default()
                .no_sys_header(true)
                .output_ty(moonutil::compiler_flags::OutputType::Executable)
                .opt_level(moonutil::compiler_flags::OptLevel::None)
                .debug_info(false)
                .link_moonbitrun(false) // if use tcc, we cannot link moonbitrun
                .define_use_shared_runtime_macro(false)
                .build()
                .unwrap(),
            &[],
            &[
                moon_lib_path.join("runtime.c").display().to_string(),
                output_wasm_or_js_path.display().to_string(),
            ],
            &output_wasm_or_js_path
                .parent()
                .unwrap()
                .display()
                .to_string(),
            &output_wasm_or_js_path
                .with_extension("exe")
                .display()
                .to_string(),
        );

        Some(cc_cmd)
    } else {
        None
    };

    let moonc = &*moonutil::BINARIES.moonc;
    if cli.dry_run {
        let moonc = moonc.display();
        println!("{moonc} {}", build_package_command.join(" "));
        println!("{moonc} {}", link_core_command.join(" "));
        if let Some(compile_exe_command) = compile_exe_command {
            println!("{}", compile_exe_command.join(" "));
        }
        if !cmd.build_only {
            match target_backend {
                TargetBackend::Wasm | TargetBackend::WasmGC => {
                    println!(
                        "{} {}",
                        moonutil::BINARIES.moonrun.display(),
                        output_wasm_or_js_path.display()
                    );
                }
                TargetBackend::Js => {
                    println!(
                        "{} {}",
                        moonutil::BINARIES.node_or_default().display(),
                        output_wasm_or_js_path.display()
                    );
                }
                TargetBackend::Native | TargetBackend::LLVM => {
                    if !use_tcc_run {
                        println!("{}", output_wasm_or_js_path.with_extension("exe").display());
                    }
                }
            }
        }
        return Ok(0);
    }

    let moonc_build_package = std::process::Command::new(moonc)
        .args(&build_package_command)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()?
        .wait()?;

    if !moonc_build_package.success() {
        bail!(
            "failed to run: {} {}",
            moonc.display(),
            build_package_command.join(" ")
        )
    }

    let moonc_link_core = std::process::Command::new(moonc)
        .args(&link_core_command)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()?
        .wait()?;

    if !moonc_link_core.success() {
        bail!(
            "failed to run: {} {}",
            moonc.display(),
            link_core_command.join(" ")
        )
    }

    if let Some(compile_exe_command) = compile_exe_command {
        let compile_exe = std::process::Command::new(&compile_exe_command[0])
            .args(&compile_exe_command[1..])
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()?
            .wait()?;

        if !compile_exe.success() {
            bail!("failed to run: {}", compile_exe_command.join(" "));
        }
    }

    if cmd.build_only {
        let test_artifacts = TestArtifacts {
            artifacts_path: vec![output_wasm_or_js_path],
        };
        println!("{}", serde_json_lenient::to_string(&test_artifacts)?);
        return Ok(0);
    }

    trace::scope("run", || match target_backend {
        TargetBackend::Wasm | TargetBackend::WasmGC => {
            moonbuild::build::run_wat(&output_wasm_or_js_path, &cmd.args, cli.verbose)
        }
        TargetBackend::Js => {
            moonbuild::build::run_js(&output_wasm_or_js_path, &cmd.args, cli.verbose)
        }
        TargetBackend::Native | TargetBackend::LLVM => {
            if !use_tcc_run {
                moonbuild::build::run_native(
                    &output_wasm_or_js_path.with_extension("exe"),
                    &cmd.args,
                    cli.verbose,
                )
            } else {
                Ok(())
            }
        }
    })?;

    Ok(0)
}

#[instrument(skip_all)]
pub fn run_run_internal(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    if cli.unstable_feature.rupes_recta {
        run_run_rr(cli, cmd)
    } else {
        run_run_internal_legacy(cli, cmd)
    }
}

#[instrument(skip_all)]
fn run_run_rr(cli: &UniversalFlags, cmd: RunSubcommand) -> Result<i32, anyhow::Error> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let input_path = cmd.package_or_mbt_file.clone();
    let mut preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        &target_dir,
        Release,
        RunMode::Run,
    );
    preconfig.try_tcc_run = !cli.dry_run;

    let value_tracing = cmd.build_flags.enable_value_tracing;
    let (build_meta, build_graph) = rr_build::plan_build(
        preconfig,
        &cli.unstable_feature,
        &source_dir,
        &target_dir,
        Box::new(|r, _tb| calc_user_intent(&input_path, &source_dir, r, value_tracing)),
    )?;
    rr_run_from_plan(
        cli,
        &cmd,
        &source_dir,
        &target_dir,
        &build_meta,
        build_graph,
    )
}

#[instrument(level = Level::DEBUG, skip_all)]
fn get_run_cmd(
    build_meta: &rr_build::BuildMeta,
    argv: &[String],
) -> Result<CommandGuard, anyhow::Error> {
    let (_, artifact) = build_meta
        .artifacts
        .first()
        .expect("Expected exactly one build node emitted by `calc_user_intent`");
    let executable = artifact
        .artifacts
        .first()
        .expect("Expected exactly one executable as the output of the build node");
    let mut cmd = crate::run::command_for(build_meta.target_backend, executable, None)?;
    cmd.command.args(argv);
    Ok(cmd)
}

#[instrument(level = Level::DEBUG, skip_all)]
fn calc_user_intent(
    input_path: &str,
    source_dir: &Path,
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    value_tracing: bool,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    // The legacy impl says the input path is based on `source_dir`, while
    // if we want to match the behavior of other commands we need it to be based
    // on current dir. We temporarily accept both behaviors here.
    // TODO: Decide on one behavior and remove the other.
    let (dir, _filename) = match crate::filter::canonicalize_with_filename(Path::new(input_path)) {
        Ok((dir, filename)) => (dir, filename),
        Err(e) => {
            let backup_path = source_dir.join(input_path);
            crate::filter::canonicalize_with_filename(&backup_path).map_err(|e2| {
                anyhow::anyhow!(
                    "Failed to canonicalize input path based on current working directory: {}\n\
                    Also failed to canonicalize based on source directory `{}`: {}\n\
                    Please make sure the path exists.",
                    e,
                    source_dir.display(),
                    e2
                )
            })?
        }
    };
    let pkg = crate::filter::filter_pkg_by_dir(resolve_output, &dir)?;

    // check whether it's a main package

    if !resolve_output.pkg_dirs.get_package(pkg).raw.is_main {
        bail!("`{}` is not a main package", input_path);
    }

    if value_tracing {
        Ok((
            vec![UserIntent::Run(pkg)],
            InputDirective {
                value_tracing: Some(pkg),
                ..Default::default()
            },
        )
            .into())
    } else {
        Ok(vec![UserIntent::Run(pkg)].into())
    }
}

#[instrument(skip_all)]
fn run_run_internal_legacy(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    let moon_pkg_json_exist = std::env::current_dir()?
        .join(&cmd.package_or_mbt_file)
        .parent()
        .is_some_and(is_moon_pkg_exist);
    if cmd.package_or_mbt_file.ends_with(".mbt") && !moon_pkg_json_exist {
        return run_single_mbt_file(cli, cmd);
    }

    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    // Run moon install before build
    let (resolved_env, dir_sync_result) = auto_sync(
        &source_dir,
        &cmd.auto_sync_flags,
        &RegistryConfig::load(),
        cli.quiet,
        true, // Legacy don't need std injection
    )?;

    let run_mode = RunMode::Run;
    let moonc_opt = super::get_compiler_flags(&source_dir, &cmd.build_flags)?;

    let raw_target_dir = target_dir.to_path_buf();
    let target_dir = mk_arch_mode_dir(&source_dir, &target_dir, &moonc_opt, run_mode)?;
    let _lock = FileLock::lock(&target_dir)?;

    if moonc_opt.link_opt.output_format == OutputFormat::Wat {
        bail!("`--output-wat` is not supported for `run`");
    }

    let sort_input = cmd.build_flags.sort_input;

    // run .mbt inside a package should run as a package
    let package_path = if cmd.package_or_mbt_file.ends_with(".mbt") {
        // `package_path` based on `source_dir`
        let full_path = std::env::current_dir()?.join(cmd.package_or_mbt_file);
        dunce::canonicalize(&full_path)
            .with_context(|| format!("can't canonicalize {}", full_path.display()))?
            .parent()
            .unwrap()
            .strip_prefix(&source_dir)?
            .display()
            .to_string()
    } else {
        cmd.package_or_mbt_file
    };
    let package = source_dir.join(&package_path);
    if !is_moon_pkg_exist(&package) {
        bail!("{} is not a package", package_path);
    }

    let pkg = moonutil::common::read_package_desc_file_in_dir(&package)?;
    if !pkg.is_main {
        bail!("`{}` is not a main package", package_path);
    }
    let moonbuild_opt = MoonbuildOpt {
        source_dir,
        raw_target_dir,
        target_dir,
        sort_input,
        run_mode,
        args: cmd.args.clone(),
        quiet: true,
        verbose: cli.verbose,
        build_graph: cli.build_graph,
        test_opt: None,
        check_opt: None,
        build_opt: None,
        fmt_opt: None,
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

    let pkg = module.get_package_by_path_mut(&package).ok_or_else(|| {
        crate::filter::report_package_not_found(
            &package,
            &resolved_env,
            &dir_sync_result,
            resolved_env.input_module_ids(),
        )
    })?;
    pkg.enable_value_tracing = cmd.build_flags.enable_value_tracing;

    moonutil::common::set_native_backend_link_flags(
        run_mode,
        moonc_opt.build_opt.target_backend,
        &mut module,
    )?;

    if cli.dry_run {
        return dry_run::print_commands(&module, &moonc_opt, &moonbuild_opt);
    }

    entry::run_run_with_lock(
        &package_path,
        &moonc_opt,
        &moonbuild_opt,
        &module,
        cmd.build_only,
        Some(_lock),
    )
}

#[instrument(level = Level::DEBUG, skip_all)]
fn run_single_file_rr(cli: &UniversalFlags, mut cmd: RunSubcommand) -> anyhow::Result<i32> {
    let current_dir = std::env::current_dir()?;
    let input_path = dunce::canonicalize(current_dir.join(&cmd.package_or_mbt_file))?;
    let source_dir = input_path.parent().unwrap().to_path_buf();
    let raw_target_dir = source_dir.join(BUILD_DIR);
    std::fs::create_dir_all(&raw_target_dir).context("failed to create target directory")?;

    let value_tracing = cmd.build_flags.enable_value_tracing;

    cmd.build_flags.populate_target_backend_from_list()?;

    // Resolve single-file project (synthesized package around the file)
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        RegistryConfig::load(),
        false,
        cmd.build_flags.enable_coverage,
    );
    let (resolved, backend) = moonbuild_rupes_recta::resolve::resolve_single_file_project(
        &resolve_cfg,
        &input_path,
        true,
    )?;

    let mut preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags.clone().with_default_target_backend(backend),
        &raw_target_dir,
        OptLevel::Debug,
        RunMode::Run,
    );
    // Match legacy behavior: allow tcc-run for Native debug runs in RR single-file if not dry-run
    preconfig.try_tcc_run = !cli.dry_run;

    // Plan build with a single UserIntent::Run for the synthesized package
    let (build_meta, build_graph) = rr_build::plan_build_from_resolved(
        preconfig,
        &cli.unstable_feature,
        &raw_target_dir,
        Box::new(move |r, _tb| {
            let m_packages = r
                .pkg_dirs
                .packages_for_module(r.local_modules()[0])
                .expect("Local module must exist");
            let pkg = *m_packages
                .iter()
                .next()
                .expect("Single-file project must synthesize exactly one package")
                .1;

            let directive = if value_tracing {
                rr_build::build_patch_directive_for_package(pkg, false, Some(pkg), None, false)?
            } else {
                Default::default()
            };

            Ok((vec![UserIntent::Run(pkg)], directive).into())
        }),
        resolved,
    )?;

    rr_run_from_plan(
        cli,
        &cmd,
        &source_dir,
        &raw_target_dir,
        &build_meta,
        build_graph,
    )
}

#[instrument(level = Level::DEBUG, skip_all)]
fn rr_run_from_plan(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    build_meta: &rr_build::BuildMeta,
    build_graph: rr_build::BuildInput,
) -> Result<i32, anyhow::Error> {
    if cli.dry_run {
        rr_build::print_dry_run(
            &build_graph,
            build_meta.artifacts.values(),
            source_dir,
            target_dir,
        );

        let run_cmd = get_run_cmd(build_meta, &cmd.args)?;
        rr_build::dry_print_command(run_cmd.command.as_std(), source_dir, false);
        return Ok(0);
    }

    let _lock = FileLock::lock(target_dir)?;
    // Generate all_pkgs.json for indirect dependency resolution
    rr_build::generate_all_pkgs_json(target_dir, build_meta, RunMode::Run)?;

    let build_config =
        BuildConfig::from_flags(&cmd.build_flags, &cli.unstable_feature, cli.verbose);
    let build_result = rr_build::execute_build(&build_config, build_graph, target_dir)?;

    if !build_result.successful() {
        return Ok(build_result.return_code_for_success());
    }
    let run_cmd = get_run_cmd(build_meta, &cmd.args)?;
    if cli.verbose {
        rr_build::dry_print_command(run_cmd.command.as_std(), source_dir, true);
    }

    // Release the lock before spawning the subprocess
    drop(_lock);

    if cmd.build_only {
        let test_artifacts = TestArtifacts {
            artifacts_path: build_meta
                .artifacts
                .values()
                .flat_map(|artifact| artifact.artifacts.clone())
                .collect(),
        };
        println!("{}", serde_json_lenient::to_string(&test_artifacts)?);
        return Ok(0);
    }

    let res = default_rt()
        .context("Failed to create runtime")?
        .block_on(crate::run::run(&mut [], false, run_cmd.command))
        .context("failed to run command")?;

    if let Some(code) = res.code() {
        Ok(code)
    } else {
        bail!("Command exited without a return code")
    }
}
