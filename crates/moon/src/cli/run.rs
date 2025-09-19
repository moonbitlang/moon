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

use std::path::PathBuf;

use anyhow::{bail, Context};
use moonbuild::dry_run;
use moonbuild::entry;
use moonbuild_rupes_recta::model::BuildPlanNode;
use moonbuild_rupes_recta::model::BuildTarget;
use mooncake::pkg::sync::auto_sync;
use moonutil::common::lower_surface_targets;
use moonutil::common::FileLock;
use moonutil::common::PrePostBuild;
use moonutil::common::RunMode;
use moonutil::common::SurfaceTarget;
use moonutil::common::TargetBackend;
use moonutil::common::TestArtifacts;
use moonutil::common::MOONBITLANG_CORE;
use moonutil::common::MOON_PKG_JSON;
use moonutil::common::{MoonbuildOpt, OutputFormat};
use moonutil::cond_expr::OptLevel::Release;
use moonutil::dirs::check_moon_pkg_exist;
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::dirs::PackageDirs;
use moonutil::moon_dir::MOON_DIRS;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use n2::trace;
use tracing::{instrument, Level};

use crate::rr_build;
use crate::rr_build::preconfig_compile;
use crate::rr_build::{BuildConfig, CalcUserIntentOutput};
use crate::run::default_rt;
use crate::run::CommandGuard;

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

    let output_artifact_path = mbt_file_parent_path.join("target");

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

    if cli.dry_run {
        println!("moonc {}", build_package_command.join(" "));
        println!("moonc {}", link_core_command.join(" "));
        if let Some(compile_exe_command) = compile_exe_command {
            println!("{}", compile_exe_command.join(" "));
        }
        if !cmd.build_only {
            match target_backend {
                TargetBackend::Wasm | TargetBackend::WasmGC => {
                    println!("moonrun {}", output_wasm_or_js_path.display());
                }
                TargetBackend::Js => {
                    println!("node {}", output_wasm_or_js_path.display());
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

    let moonc_build_package = std::process::Command::new("moonc")
        .args(&build_package_command)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()?
        .wait()?;

    if !moonc_build_package.success() {
        bail!("failed to run: moonc {}", build_package_command.join(" "))
    }

    let moonc_link_core = std::process::Command::new("moonc")
        .args(&link_core_command)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()?
        .wait()?;

    if !moonc_link_core.success() {
        bail!("failed to run: moonc {}", link_core_command.join(" "))
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

    let input_path = cmd.package_or_mbt_file;
    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        &target_dir,
        Release,
        RunMode::Run,
    );
    let (build_meta, build_graph) = rr_build::plan_build(
        preconfig,
        &cli.unstable_feature,
        &source_dir,
        &target_dir,
        Box::new(|r, m| calc_user_intent(&input_path, r, m)),
    )?;
    if cli.dry_run {
        // Print build commands
        rr_build::print_dry_run(
            &build_graph,
            build_meta.artifacts.values(),
            &source_dir,
            &target_dir,
        );

        let cmd = get_run_cmd(&build_meta)?;
        rr_build::dry_print_command(cmd.command.as_std());

        Ok(0)
    } else {
        let build_result = rr_build::execute_build(
            &BuildConfig::from_flags(&cmd.build_flags, &cli.unstable_feature),
            build_graph,
            &target_dir,
        )?;

        if !build_result.successful() {
            return Ok(build_result.return_code_for_success());
        }

        let cmd = get_run_cmd(&build_meta)?;

        // FIXME: Simplify this part
        let res = default_rt()
            .context("Failed to create runtime")?
            .block_on(crate::run::run(&mut [], true, cmd.command))
            .context("failed to run command")?;

        if let Some(code) = res.code() {
            Ok(code)
        } else {
            bail!("Command exited without a return code");
        }
    }
}

#[instrument(level = Level::DEBUG, skip_all)]
fn get_run_cmd(build_meta: &rr_build::BuildMeta) -> Result<CommandGuard, anyhow::Error> {
    let (_, artifact) = build_meta
        .artifacts
        .first()
        .expect("Expected exactly one build node emitted by `calc_user_intent`");
    let executable = artifact
        .artifacts
        .first()
        .expect("Expected exactly one executable as the output of the build node");
    let cmd = crate::run::command_for(build_meta.target_backend, executable, None)?;
    Ok(cmd)
}

#[instrument(level = Level::DEBUG, skip_all)]
fn calc_user_intent(
    input_path: &str,
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    main_modules: &[moonutil::mooncakes::ModuleId],
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    // `moon run` requires a path relative to CWD being provided. The path may
    // either be a MoonBit source code file, or a path to a module directory.
    //
    // We currently assume that this is *not* a single file to run (which is
    // another separate problem on its own). This leaves us with the following
    // best-effort solution to determine the package to run:
    //
    // 1. Canonicalize the `path` to its absolute form.
    // 2. Get two values: the `path` itself, and its `parent`.
    // 3. For each package, check if its path match either of the two paths provided.
    // 4. If the `path` itself is matched, the package matching it is the target one to run.
    // 5. Else if the `parent` path is matched, the package matching it is the target one to run.
    // 6. Otherwise, return an error.
    let input_path = PathBuf::from(input_path);
    let input_path =
        dunce::canonicalize(input_path).context("Failed to canonicalize input file path")?;
    let input_path_parent = input_path.parent();

    let mut found_path = None;
    let mut found_path_parent = None;
    for m in main_modules {
        for p in resolve_output
            .pkg_dirs
            .packages_for_module(*m)
            .expect("Module should exist")
            .values()
        {
            let pkg = resolve_output.pkg_dirs.get_package(*p);
            if pkg.root_path == input_path {
                found_path = Some(*p);
            } else if let Some(parent) = input_path_parent {
                if pkg.root_path == parent {
                    found_path_parent = Some(*p);
                }
            }
        }
    }

    let found = found_path.or(found_path_parent);
    if let Some(pkg_id) = found {
        Ok(vec![BuildPlanNode::make_executable(BuildTarget {
            package: pkg_id,
            kind: moonbuild_rupes_recta::model::TargetKind::Source,
        })]
        .into())
    } else {
        Err(anyhow::anyhow!(
            "Cannot find package to build based on input path `{}`",
            input_path.display()
        ))
    }
}

#[instrument(skip_all)]
fn run_run_internal_legacy(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    let moon_pkg_json_exist = std::env::current_dir()?
        .join(&cmd.package_or_mbt_file)
        .parent()
        .is_some_and(|p| p.join(MOON_PKG_JSON).exists());
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
    if !check_moon_pkg_exist(&package) {
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
        output_json: false,
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

    let pkg = module.get_package_by_path_mut(&package).unwrap();
    pkg.enable_value_tracing = cmd.build_flags.enable_value_tracing;

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
    let result = entry::run_run(
        &package_path,
        &moonc_opt,
        &moonbuild_opt,
        &module,
        cmd.build_only,
    );
    if trace_flag {
        trace::close();
    }
    result
}
