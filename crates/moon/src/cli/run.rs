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

use std::{io::Read, path::Path};

use anyhow::{Context, bail};
use moonbuild_rupes_recta::build_plan::InputDirective;
use moonbuild_rupes_recta::intent::UserIntent;
use moonutil::common::{FileLock, RunMode, TargetBackend, TestArtifacts, is_moon_pkg_exist};
use moonutil::dirs::{PackageDirs, ProjectProbe};
use moonutil::mooncakes::sync::AutoSyncFlags;
use tracing::{Level, instrument};

use crate::filter::ensure_package_supports_backend;
use crate::rr_build;
use crate::rr_build::preconfig_compile;
use crate::rr_build::{BuildConfig, CalcUserIntentOutput};
use crate::run::default_rt;
use crate::user_diagnostics::UserDiagnostics;

use super::{BuildFlags, UniversalFlags};

/// Run a main package
#[derive(Debug, clap::Parser, Clone)]
#[clap(group = clap::ArgGroup::new("run_source").required(true).multiple(false))]
pub(crate) struct RunSubcommand {
    /// The package, .mbt/.mbtx file, or `-` to read `.mbtx` source from stdin
    #[clap(group = "run_source")]
    pub package_or_mbt_file: Option<String>,

    /// Run `.mbtx` source passed in as a string
    #[clap(
        short = 'c',
        short_alias = 'e',
        value_name = "SCRIPT",
        group = "run_source"
    )]
    pub command: Option<String>,

    #[clap(flatten)]
    pub build_flags: BuildFlags,

    /// The arguments provided to the program to be run
    #[clap(trailing_var_arg = true, num_args = 0.., allow_hyphen_values = true)]
    pub args: Vec<String>,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Only build, do not run the code
    #[clap(long)]
    pub build_only: bool,
}

fn run_stdin_source_as_single_file(
    cli: &UniversalFlags,
    cmd: RunSubcommand,
) -> anyhow::Result<i32> {
    let mut source = String::new();
    std::io::stdin()
        .read_to_string(&mut source)
        .context("failed to read `.mbtx` source from stdin")?;

    run_source_as_single_file(cli, cmd, source, "stdin.mbtx", "stdin")
}

fn run_inline_source_as_single_file(
    cli: &UniversalFlags,
    cmd: RunSubcommand,
) -> anyhow::Result<i32> {
    let source = cmd
        .command
        .clone()
        .expect("inline script should be present when `moon run -c` is selected");

    run_source_as_single_file(cli, cmd, source, "command.mbtx", "command")
}

fn run_source_as_single_file(
    cli: &UniversalFlags,
    cmd: RunSubcommand,
    source: String,
    temp_name: &str,
    source_name: &str,
) -> anyhow::Result<i32> {
    let temp_dir = tempfile::TempDir::new()
        .with_context(|| format!("failed to create temporary directory for {source_name} run"))?;
    let input_path = temp_dir.path().join(temp_name);
    std::fs::write(&input_path, source).with_context(|| {
        format!(
            "failed to write temporary {source_name} source file: {}",
            input_path.display()
        )
    })?;

    let RunSubcommand {
        command: _,
        build_flags,
        args,
        auto_sync_flags,
        build_only,
        ..
    } = cmd;
    let cmd = RunSubcommand {
        package_or_mbt_file: Some(input_path.to_string_lossy().into_owned()),
        command: None,
        build_flags,
        args,
        auto_sync_flags,
        build_only,
    };
    let result = run_single_file_from_arg(cli, cmd);
    drop(temp_dir);
    result
}

fn reject_manifest_path_for_run(cli: &UniversalFlags) -> anyhow::Result<()> {
    if cli.source_tgt_dir.manifest_path.is_some() {
        bail!(
            "`--manifest-path` is no longer supported for `moon run`. Use `moon -C <project-dir> run ...` instead."
        );
    }
    Ok(())
}

fn resolve_run_start_dir(input: &str) -> anyhow::Result<std::path::PathBuf> {
    let input_path =
        dunce::canonicalize(input).with_context(|| format!("failed to resolve path `{input}`"))?;
    if input_path.is_dir() {
        Ok(input_path)
    } else {
        input_path
            .parent()
            .context("run input path has no parent directory")
            .map(Path::to_path_buf)
    }
}

#[instrument(skip_all)]
pub(crate) fn run_run(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    reject_manifest_path_for_run(cli)?;

    if cmd.command.is_some() {
        return run_inline_source_as_single_file(cli, cmd);
    }

    let input = cmd
        .package_or_mbt_file
        .as_deref()
        .expect("run source is required by clap");
    if input == "-" {
        return run_stdin_source_as_single_file(cli, cmd);
    }
    let is_mbt = input.ends_with(".mbt");
    let is_mbtx = input.ends_with(".mbtx");
    let run_start_dir = resolve_run_start_dir(input)?;

    let mut query = cli
        .source_tgt_dir
        .query_from(&run_start_dir, cli.workspace_env.clone())?;
    match query.probe_project()? {
        ProjectProbe::Found(_) => {
            if is_mbtx {
                return run_single_file_from_arg(cli, cmd);
            }
            if is_mbt {
                let moon_pkg_json_exist =
                    std::fs::metadata(input)?.is_file() && is_moon_pkg_exist(&run_start_dir);
                if !moon_pkg_json_exist {
                    return run_single_file_from_arg(cli, cmd);
                }
            }
        }
        ProjectProbe::NotFound(not_found) => {
            if is_mbt || is_mbtx {
                return run_single_file_from_arg(cli, cmd);
            }
            return Err(not_found.into_error().into());
        }
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
    let run_start_dir = resolve_run_start_dir(
        cmd.package_or_mbt_file
            .as_deref()
            .expect("package run planning requires a positional input"),
    )?;
    let PackageDirs {
        source_dir,
        target_dir,
        mooncakes_dir,
        project_manifest_path,
    } = cli
        .source_tgt_dir
        .query_from(&run_start_dir, cli.workspace_env.clone())?
        .package_dirs()?;

    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        !cmd.build_flags.std(),
        cmd.build_flags.enable_coverage,
        cli.workspace_env.clone(),
    )
    .with_project_manifest_path(project_manifest_path.as_deref());
    let resolve_output = moonbuild_rupes_recta::resolve(&resolve_cfg, &source_dir, &mooncakes_dir)?;
    let (build_meta, build_graph) = plan_run_rr_from_resolved(
        cli,
        &cmd,
        &source_dir,
        &target_dir,
        selected_target_backend,
        resolve_output,
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

pub(crate) fn plan_run_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
) -> anyhow::Result<(rr_build::BuildMeta, rr_build::BuildInput)> {
    let mut preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        target_dir,
        RunMode::Run,
    );
    preconfig.try_tcc_run = !cli.dry_run;

    let input_path = cmd
        .package_or_mbt_file
        .clone()
        .expect("package run planning requires a positional input");
    let value_tracing = cmd.build_flags.enable_value_tracing;
    rr_build::plan_build_from_resolved(
        preconfig,
        &cli.unstable_feature,
        target_dir,
        UserDiagnostics::from_flags(cli),
        Box::new(|resolved, target_backend| {
            calc_user_intent(
                &input_path,
                source_dir,
                resolved,
                value_tracing,
                target_backend,
            )
        }),
        resolve_output,
    )
}

#[instrument(level = Level::DEBUG, skip_all)]
fn get_run_cmd(build_meta: &rr_build::BuildMeta, argv: &[String]) -> tokio::process::Command {
    let (_, artifact) = build_meta
        .artifacts
        .first()
        .expect("Expected exactly one build node emitted by `calc_user_intent`");
    let executable = artifact
        .artifacts
        .first()
        .expect("Expected exactly one executable as the output of the build node");
    let mut cmd = crate::run::command_for(build_meta.target_backend, executable, None);
    cmd.args(argv);
    cmd
}

#[instrument(level = Level::DEBUG, skip_all)]
fn calc_user_intent(
    input_path: &str,
    _source_dir: &Path,
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    value_tracing: bool,
    target_backend: TargetBackend,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let (dir, _filename) = crate::filter::canonicalize_with_filename(Path::new(input_path))?;
    let pkg = crate::filter::filter_pkg_by_dir(resolve_output, &dir)?;

    // check whether it's a main package

    if !resolve_output.pkg_dirs.get_package(pkg).raw.is_main {
        bail!("`{}` is not a main package", input_path);
    }
    ensure_package_supports_backend(resolve_output, pkg, target_backend)?;

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
fn run_single_file_from_arg(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    let single_file_dirs = cli.source_tgt_dir.single_file_package_dirs(
        cmd.package_or_mbt_file
            .as_deref()
            .expect("single-file run from arg requires a positional input path"),
    )?;
    let target_dir = single_file_dirs.package_dirs.target_dir;
    let mooncakes_dir = single_file_dirs.package_dirs.mooncakes_dir;

    run_single_file_rr(
        cli,
        cmd,
        single_file_dirs.package_dirs.source_dir,
        target_dir,
        mooncakes_dir,
        single_file_dirs.file_path,
    )
}

#[instrument(level = Level::DEBUG, skip_all)]
fn run_single_file_rr(
    cli: &UniversalFlags,
    cmd: RunSubcommand,
    source_dir: std::path::PathBuf,
    target_dir: std::path::PathBuf,
    mooncakes_dir: std::path::PathBuf,
    input_path: std::path::PathBuf,
) -> anyhow::Result<i32> {
    std::fs::create_dir_all(&target_dir).context("failed to create target directory")?;

    let value_tracing = cmd.build_flags.enable_value_tracing;

    let selected_target_backend = cmd.build_flags.resolve_single_target_backend()?;

    // Resolve single-file project (synthesized package around the file)
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        false,
        cmd.build_flags.enable_coverage,
        cli.workspace_env.clone(),
    );
    let (resolved, backend) = moonbuild_rupes_recta::resolve::resolve_single_file_project(
        &resolve_cfg,
        target_dir.as_path(),
        &mooncakes_dir,
        &input_path,
        true,
    )?;
    let selected_target_backend = selected_target_backend.or(backend);

    let mut preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        &target_dir,
        RunMode::Run,
    );
    // Match legacy behavior: allow tcc-run for Native debug runs in RR single-file if not dry-run
    preconfig.try_tcc_run = !cli.dry_run;

    // Plan build with a single UserIntent::Run for the synthesized package
    let (build_meta, build_graph) = rr_build::plan_build_from_resolved(
        preconfig,
        &cli.unstable_feature,
        &target_dir,
        UserDiagnostics::from_flags(cli),
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
        &target_dir,
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

        let run_cmd = get_run_cmd(build_meta, &cmd.args);
        rr_build::dry_print_command(run_cmd.as_std(), source_dir, false);
        return Ok(0);
    }

    let _lock = FileLock::lock(target_dir)?;
    // Generate all_pkgs.json for indirect dependency resolution
    rr_build::generate_all_pkgs_json(target_dir, build_meta, RunMode::Run)?;

    let build_config = BuildConfig::from_flags(
        &cmd.build_flags,
        &cli.unstable_feature,
        cli.verbose,
        UserDiagnostics::from_flags(cli),
    );
    let build_result = rr_build::execute_build(&build_config, build_graph, target_dir)?;

    if !build_result.successful() {
        return Ok(build_result.return_code_for_success());
    }
    let run_cmd = get_run_cmd(build_meta, &cmd.args);
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
            test_filter_args: vec![],
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
