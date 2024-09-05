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

use anyhow::{bail, Context};
use moonbuild::dry_run;
use moonbuild::entry;
use mooncake::pkg::sync::auto_sync;
use moonutil::common::lower_surface_targets;
use moonutil::common::FileLock;
use moonutil::common::RunMode;
use moonutil::common::SurfaceTarget;
use moonutil::common::TargetBackend;
use moonutil::common::TestArtifacts;
use moonutil::common::MOONBITLANG_CORE;
use moonutil::common::MOON_PKG_JSON;
use moonutil::common::{MoonbuildOpt, OutputFormat};
use moonutil::dirs::check_moon_pkg_exist;
use moonutil::dirs::mk_arch_mode_dir;
use moonutil::dirs::PackageDirs;
use moonutil::mooncakes::sync::AutoSyncFlags;
use moonutil::mooncakes::RegistryConfig;
use n2::trace;

use super::{BuildFlags, UniversalFlags};

/// Run a main package
#[derive(Debug, clap::Parser, Clone)]
pub struct RunSubcommand {
    /// The package or .mbt file to run
    pub package_or_mbt_file: String,

    #[clap(flatten)]
    pub build_flags: BuildFlags,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    pub args: Vec<String>,

    #[clap(long)]
    pub build_only: bool,
}

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
        .join(format!("{}.core", file_name))
        .display()
        .to_string());

    let output_wasm_or_js_path =
        output_artifact_path.join(format!("{}.{}", file_name, target_backend.to_extension()));

    let pkg_name = "moon/run/single";
    let build_package_command = [
        "build-package",
        &mbt_file_path.display().to_string(),
        "-o",
        output_core_path,
        "-std-path",
        core_bundle_path.to_str().unwrap(),
        "-is-main",
        "-pkg",
        pkg_name,
        "-g",
        "-source-map",
        "-target",
        target_backend.to_flag(),
    ];
    let link_core_command = [
        "link-core",
        &moonutil::moon_dir::core_core(target_backend)
            .display()
            .to_string(),
        &(output_artifact_path
            .join(format!("{}.core", file_name))
            .display()
            .to_string()),
        "-o",
        &output_wasm_or_js_path.display().to_string(),
        "-pkg-sources",
        &format!("{}:{}", pkg_name, mbt_file_parent_path.display()),
        "-pkg-sources",
        &format!(
            "{}:{}",
            MOONBITLANG_CORE,
            moonutil::moon_dir::core().display()
        ),
        "-g",
        "-source-map",
        "-target",
        target_backend.to_flag(),
    ];

    if cli.dry_run {
        println!("moonc {}", build_package_command.join(" "));
        println!("moonc {}", link_core_command.join(" "));
        if !cmd.build_only {
            let runner = match target_backend {
                TargetBackend::Wasm | TargetBackend::WasmGC => "moonrun",
                TargetBackend::Js => "node",
            };
            println!("{} {}", runner, output_wasm_or_js_path.display());
        }
        return Ok(0);
    }

    let moonc_build_package = std::process::Command::new("moonc")
        .args(build_package_command)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()?
        .wait()?;

    if !moonc_build_package.success() {
        bail!("failed to run: moonc {}", build_package_command.join(" "))
    }

    let moonc_link_core = std::process::Command::new("moonc")
        .args(link_core_command)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()?
        .wait()?;

    if !moonc_link_core.success() {
        bail!("failed to run: moonc {}", link_core_command.join(" "))
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
            moonbuild::build::run_wat(&output_wasm_or_js_path, &cmd.args)
        }
        TargetBackend::Js => moonbuild::build::run_js(&output_wasm_or_js_path, &cmd.args),
    })?;

    Ok(0)
}

pub fn run_run_internal(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    let moon_pkg_json_exist = std::env::current_dir()?
        .join(&cmd.package_or_mbt_file)
        .parent()
        .map_or(false, |p| p.join(MOON_PKG_JSON).exists());
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

    let mut moonc_opt = super::get_compiler_flags(&source_dir, &cmd.build_flags)?;
    let run_mode = RunMode::Run;
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
        target_dir,
        sort_input,
        run_mode,
        args: cmd.args.clone(),
        quiet: true,
        verbose: cli.verbose,
        build_graph: cli.build_graph,
        ..Default::default()
    };

    let module = moonutil::scan::scan(
        false,
        &resolved_env,
        &dir_sync_result,
        &moonc_opt,
        &moonbuild_opt,
    )?;
    moonc_opt.build_opt.warn_lists = module
        .packages
        .iter()
        .map(|(name, pkg)| (name.clone(), pkg.warn_list.clone()))
        .collect();
    moonc_opt.build_opt.alert_lists = module
        .packages
        .iter()
        .map(|(name, pkg)| (name.clone(), pkg.alert_list.clone()))
        .collect();
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
