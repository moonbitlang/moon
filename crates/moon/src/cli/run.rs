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
use moonbuild_rupes_recta::build_plan::InputDirective;
use moonbuild_rupes_recta::intent::UserIntent;
use moonutil::common::{
    BUILD_DIR, FileLock, RunMode, TargetBackend, TestArtifacts, is_moon_pkg_exist,
};
use moonutil::dirs::PackageDirs;
use moonutil::mooncakes::RegistryConfig;
use moonutil::mooncakes::sync::AutoSyncFlags;
use tracing::{Level, instrument};

use crate::rr_build;
use crate::rr_build::preconfig_compile;
use crate::rr_build::{BuildConfig, CalcUserIntentOutput};
use crate::run::default_rt;

use super::{BuildFlags, UniversalFlags};

/// Run a main package
#[derive(Debug, clap::Parser, Clone)]
pub(crate) struct RunSubcommand {
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
pub(crate) fn run_run(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    match cli.source_tgt_dir.try_into_package_dirs() {
        Ok(_) => {
            if cmd.package_or_mbt_file.ends_with(".mbt") {
                let moon_pkg_json_exist = std::env::current_dir()?
                    .join(&cmd.package_or_mbt_file)
                    .parent()
                    .is_some_and(is_moon_pkg_exist);
                if !moon_pkg_json_exist {
                    return run_single_file_rr(cli, cmd);
                }
            }
            // moon should report an error later if the source_dir doesn't
            // contain moon.pkg.json
        }
        Err(e @ moonutil::dirs::PackageDirsError::NotInProject(_)) => {
            if cmd.package_or_mbt_file.ends_with(".mbt") {
                return run_single_file_rr(cli, cmd);
            } else {
                return Err(e.into());
            }
        }
        Err(e) => return Err(e.into()),
    }

    let selected_target_backend = cmd.build_flags.resolve_single_target_backend()?;
    if selected_target_backend.is_some() {
        run_run_internal(cli, cmd, selected_target_backend)?;
        Ok(0)
    } else {
        run_run_internal(cli, cmd, selected_target_backend)
    }
}

#[instrument(skip_all)]
pub(crate) fn run_run_internal(
    cli: &UniversalFlags,
    cmd: RunSubcommand,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    let result = run_run_rr(cli, cmd, selected_target_backend);
    if crate::run::shutdown_requested() {
        return Ok(130);
    }
    result
}

#[instrument(skip_all)]
fn run_run_rr(
    cli: &UniversalFlags,
    cmd: RunSubcommand,
    selected_target_backend: Option<TargetBackend>,
) -> Result<i32, anyhow::Error> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let input_path = cmd.package_or_mbt_file.clone();
    let mut preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        &target_dir,
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
) -> Result<tokio::process::Command, anyhow::Error> {
    let (_, artifact) = build_meta
        .artifacts
        .first()
        .expect("Expected exactly one build node emitted by `calc_user_intent`");
    let executable = artifact
        .artifacts
        .first()
        .expect("Expected exactly one executable as the output of the build node");
    let mut cmd = crate::run::command_for(build_meta.target_backend, executable, None)?;
    cmd.args(argv);
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

#[instrument(level = Level::DEBUG, skip_all)]
fn run_single_file_rr(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    let current_dir = std::env::current_dir()?;
    let input_path = dunce::canonicalize(current_dir.join(&cmd.package_or_mbt_file))?;
    let source_dir = input_path.parent().unwrap().to_path_buf();
    let raw_target_dir = source_dir.join(BUILD_DIR);
    std::fs::create_dir_all(&raw_target_dir).context("failed to create target directory")?;

    let value_tracing = cmd.build_flags.enable_value_tracing;

    let selected_target_backend = cmd.build_flags.resolve_single_target_backend()?;

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
    let selected_target_backend = selected_target_backend.or(backend);

    let mut preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        &raw_target_dir,
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
        rr_build::dry_print_command(run_cmd.as_std(), source_dir, false);
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
        rr_build::dry_print_command(run_cmd.as_std(), source_dir, true);
    }

    // Release the lock before spawning the subprocess
    drop(_lock);

    if cmd.build_only {
        // Get the single executable path (same as get_run_cmd does)
        let (_, artifact) = build_meta
            .artifacts
            .first()
            .expect("Expected exactly one build node for moon run");
        let executable = artifact
            .artifacts
            .first()
            .expect("Expected exactly one executable");
        let test_artifacts = TestArtifacts {
            artifacts_path: vec![executable.clone()],
        };
        println!("{}", serde_json_lenient::to_string(&test_artifacts)?);
        return Ok(0);
    }

    let res = default_rt()
        .context("Failed to create runtime")?
        .block_on(crate::run::run(&mut [], false, run_cmd))
        .context("failed to run command")?;

    if let Some(code) = res.code() {
        Ok(code)
    } else {
        bail!("Command exited without a return code")
    }
}
