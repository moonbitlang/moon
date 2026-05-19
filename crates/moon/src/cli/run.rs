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

use std::{io::Read, path::Path, path::PathBuf};

use anyhow::{Context, bail};
use moonbuild_rupes_recta::{
    ResolveOutput, build_plan::InputDirective, intent::UserIntent, model::PackageId,
};
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

struct ResolvedRunSelection {
    package: PackageId,
}

impl ResolvedRunSelection {
    fn into_user_intent(
        self,
        input_path: &str,
        resolve_output: &ResolveOutput,
        value_tracing: bool,
        target_backend: TargetBackend,
    ) -> Result<CalcUserIntentOutput, anyhow::Error> {
        if !resolve_output
            .pkg_dirs
            .get_package(self.package)
            .raw
            .is_main
        {
            bail!("`{}` is not a main package", input_path);
        }
        ensure_package_supports_backend(resolve_output, self.package, target_backend)?;

        let directive = if value_tracing {
            InputDirective {
                value_tracing: Some(self.package),
                ..Default::default()
            }
        } else {
            InputDirective::default()
        };
        Ok((vec![UserIntent::Run(self.package)], directive).into())
    }
}

/// Run a main package
#[derive(Debug, clap::Parser, Clone)]
#[clap(group = clap::ArgGroup::new("run_source").required(true).multiple(false))]
pub(crate) struct RunSubcommand {
    /// The package, .mbt/.mbtx file, or `-` to read `.mbtx` source from stdin
    #[clap(group = "run_source")]
    pub package_or_mbt_file: Option<String>,

    /// Run `.mbtx` source passed in as a string
    #[clap(
        short = 'e',
        short_alias = 'c',
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
    #[clap(long, conflicts_with = "profile")]
    pub build_only: bool,

    /// Profile the native executable using Time Profiler on macOS
    #[clap(long)]
    pub profile: bool,
}

/// Controls how `moon run` builds the executable before it is consumed.
///
/// Normal execution preserves the existing debug-native fast path by allowing
/// `tcc -run`. Consumers that need a standalone executable, such as profilers,
/// should disable it.
pub(crate) struct BuildRunExecutableOptions {
    /// Whether native debug builds may use `RunBackend::NativeTccRun`.
    ///
    /// `NativeTccRun` executes through `tcc @rspfile` and does not provide the
    /// same standalone executable shape as the regular native backend.
    pub(crate) try_tcc_run: bool,
    /// Whether dry-run output should include the final executable invocation.
    pub(crate) print_dry_run_run_command: bool,
}

impl BuildRunExecutableOptions {
    fn for_run(cli: &UniversalFlags) -> Self {
        Self {
            try_tcc_run: !cli.dry_run,
            print_dry_run_run_command: true,
        }
    }
}

/// A built executable plus the state needed to consume it.
///
/// The build step keeps the target-directory lock alive until the caller either
/// runs the program or explicitly releases the lock. This preserves the previous
/// `moon run` behavior while allowing other consumers to reuse the same build
/// stage.
pub(crate) struct RunExecutable {
    /// Path to the executable-like artifact that should be launched or reported.
    pub(crate) executable: PathBuf,
    /// Backend-specific runner to use for this artifact.
    pub(crate) target_backend: moonbuild_rupes_recta::model::RunBackend,
    pub(crate) opt_level: moonutil::cond_expr::OptLevel,
    pub(crate) target_dir: PathBuf,
    source_dir: PathBuf,
    build_exit_code: Option<i32>,
    force_success_exit: bool,
    lock: Option<FileLock>,
}

struct BuildExecutableFromPlanOptions {
    force_success_exit: bool,
    print_dry_run_run_command: bool,
}

impl RunExecutable {
    pub(crate) fn ensure_build_success(&self) -> anyhow::Result<()> {
        if let Some(build_exit_code) = self.build_exit_code
            && build_exit_code != 0
        {
            bail!("failed to build run target; build exited with code {build_exit_code}");
        }
        Ok(())
    }

    /// Release the target-directory lock without running the executable.
    pub(crate) fn release_lock(&mut self) {
        drop(self.lock.take());
    }
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
        .expect("inline script should be present when `moon run -e` is selected");

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
        profile,
        ..
    } = cmd;
    let cmd = RunSubcommand {
        package_or_mbt_file: Some(input_path.to_string_lossy().into_owned()),
        command: None,
        build_flags,
        args,
        auto_sync_flags,
        build_only,
        profile,
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

fn build_wasm_file_executable_from_arg(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
) -> anyhow::Result<RunExecutable> {
    let input = cmd
        .package_or_mbt_file
        .as_deref()
        .expect("wasm run from arg requires a positional input path");
    let wasm_path =
        dunce::canonicalize(input).with_context(|| format!("failed to resolve path `{input}`"))?;
    if !wasm_path.is_file() {
        bail!("`{}` is not a file", input);
    }

    let print_dir = std::env::current_dir().context("failed to get current directory")?;
    if cli.dry_run {
        let mut run_cmd = crate::run::command_for(
            moonbuild_rupes_recta::model::RunBackend::WasmGC,
            &wasm_path,
            None,
        );
        run_cmd.args(&cmd.args);
        rr_build::dry_print_command(run_cmd.as_std(), &print_dir, false);
    }

    Ok(RunExecutable {
        executable: wasm_path,
        target_backend: moonbuild_rupes_recta::model::RunBackend::WasmGC,
        opt_level: moonutil::cond_expr::OptLevel::Debug,
        target_dir: print_dir.clone(),
        source_dir: print_dir,
        build_exit_code: (!cli.dry_run).then_some(0),
        force_success_exit: false,
        lock: None,
    })
}

#[instrument(skip_all)]
pub(crate) fn run_run(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    reject_manifest_path_for_run(cli)?;

    if cmd.profile {
        return super::profile::run_profiled_run(cli, cmd);
    }

    if cmd.command.is_some() {
        return run_inline_source_as_single_file(cli, cmd);
    }

    if cmd.package_or_mbt_file.as_deref() == Some("-") {
        return run_stdin_source_as_single_file(cli, cmd);
    }

    let executable = if cmd.package_or_mbt_file.as_deref().is_some_and(|input| {
        Path::new(input)
            .extension()
            .is_some_and(|ext| ext == "wasm")
            && std::fs::metadata(input).is_ok_and(|metadata| metadata.is_file())
    }) {
        build_wasm_file_executable_from_arg(cli, &cmd)?
    } else {
        build_run_executable(cli, &cmd, BuildRunExecutableOptions::for_run(cli))?
    };
    let result = run_executable(cli, &cmd, executable);
    if crate::run::shutdown_requested() {
        return Ok(130);
    }
    result
}

/// Resolve the run input, plan it, and build the executable artifact.
///
/// This handles the same package-vs-single-file selection as `moon run`, but
/// stops before executing the artifact. Callers can then run it, print
/// `--build-only` metadata, or pass it to another tool.
pub(crate) fn build_run_executable(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
    options: BuildRunExecutableOptions,
) -> anyhow::Result<RunExecutable> {
    let input = cmd
        .package_or_mbt_file
        .as_deref()
        .expect("run executable source should be materialized before building");
    if Path::new(input)
        .extension()
        .is_some_and(|extension| extension == "wasm")
        && std::fs::metadata(input).is_ok_and(|metadata| metadata.is_file())
    {
        return build_wasm_file_executable_from_arg(cli, cmd);
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
                return build_single_file_executable_from_arg(cli, cmd, options);
            }
            if is_mbt {
                let moon_pkg_json_exist =
                    std::fs::metadata(input)?.is_file() && is_moon_pkg_exist(&run_start_dir);
                if !moon_pkg_json_exist {
                    return build_single_file_executable_from_arg(cli, cmd, options);
                }
            }
        }
        ProjectProbe::NotFound(not_found) => {
            if is_mbt || is_mbtx {
                return build_single_file_executable_from_arg(cli, cmd, options);
            }
            return Err(not_found.into_error().into());
        }
    }

    let selected_target_backend = cmd.build_flags.resolve_single_target_backend()?;
    build_package_executable(cli, cmd, selected_target_backend, options)
}

#[instrument(skip_all)]
/// Build a package run target after the top-level input has been classified as
/// a package inside a MoonBit project.
fn build_package_executable(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
    selected_target_backend: Option<TargetBackend>,
    options: BuildRunExecutableOptions,
) -> Result<RunExecutable, anyhow::Error> {
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
        cmd,
        &target_dir,
        selected_target_backend,
        resolve_output,
        options.try_tcc_run,
    )?;
    build_executable_from_plan(
        cli,
        cmd,
        &source_dir,
        &target_dir,
        &build_meta,
        build_graph,
        BuildExecutableFromPlanOptions {
            force_success_exit: selected_target_backend.is_some(),
            print_dry_run_run_command: options.print_dry_run_run_command,
        },
    )
}

pub(crate) fn plan_run_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
    target_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    resolve_output: ResolveOutput,
    try_tcc_run: bool,
) -> anyhow::Result<(rr_build::BuildMeta, rr_build::BuildInput)> {
    let mut preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        target_dir,
        RunMode::Run,
    );
    preconfig.try_tcc_run = try_tcc_run;

    let input_path = cmd
        .package_or_mbt_file
        .clone()
        .expect("package run planning requires a positional input");
    let value_tracing = cmd.build_flags.enable_value_tracing;

    let selection = resolve_run_selection(&input_path, &resolve_output)?;
    let output = UserDiagnostics::from_flags(cli);
    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        output,
        &resolve_output,
    )?;
    let intent = selection.into_user_intent(
        &input_path,
        &resolve_output,
        value_tracing,
        planning_context.target_backend(),
    )?;
    rr_build::plan_resolved_build_from_intent(
        preconfig,
        &cli.unstable_feature,
        target_dir,
        output,
        planning_context,
        intent,
        resolve_output,
    )
}

#[instrument(level = Level::DEBUG, skip_all)]
fn get_run_cmd(build_meta: &rr_build::BuildMeta, argv: &[String]) -> tokio::process::Command {
    let executable = get_run_executable(build_meta);
    let mut cmd = crate::run::command_for(build_meta.target_backend, executable, None);
    cmd.args(argv);
    cmd
}

/// Extract the single executable artifact emitted for a `UserIntent::Run` plan.
fn get_run_executable(build_meta: &rr_build::BuildMeta) -> &Path {
    let (_, artifact) = build_meta
        .artifacts
        .first()
        .expect("Expected exactly one build node emitted by `calc_user_intent`");
    artifact
        .artifacts
        .first()
        .expect("Expected exactly one executable as the output of the build node")
}

#[instrument(level = Level::DEBUG, skip_all)]
fn resolve_run_selection(
    input_path: &str,
    resolve_output: &ResolveOutput,
) -> Result<ResolvedRunSelection, anyhow::Error> {
    let (dir, _filename) = crate::filter::canonicalize_with_filename(Path::new(input_path))?;
    let package = crate::filter::filter_pkg_by_dir(resolve_output, &dir)?;
    Ok(ResolvedRunSelection { package })
}

#[instrument(level = Level::DEBUG, skip_all)]
fn run_single_file_from_arg(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    let executable =
        build_single_file_executable_from_arg(cli, &cmd, BuildRunExecutableOptions::for_run(cli))?;
    run_executable(cli, &cmd, executable)
}

/// Build a standalone `.mbt`/`.mbtx` input through the synthesized single-file
/// package machinery.
fn build_single_file_executable_from_arg(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
    options: BuildRunExecutableOptions,
) -> anyhow::Result<RunExecutable> {
    let single_file_dirs = cli.source_tgt_dir.single_file_package_dirs(
        cmd.package_or_mbt_file
            .as_deref()
            .expect("single-file run from arg requires a positional input path"),
    )?;
    let target_dir = single_file_dirs.package_dirs.target_dir;
    let mooncakes_dir = single_file_dirs.package_dirs.mooncakes_dir;

    build_single_file_executable(
        cli,
        cmd,
        single_file_dirs.package_dirs.source_dir,
        target_dir,
        mooncakes_dir,
        single_file_dirs.file_path,
        options,
    )
}

#[instrument(level = Level::DEBUG, skip_all)]
/// Build a run executable from already-resolved single-file package directories.
fn build_single_file_executable(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
    source_dir: std::path::PathBuf,
    target_dir: std::path::PathBuf,
    mooncakes_dir: std::path::PathBuf,
    input_path: std::path::PathBuf,
    options: BuildRunExecutableOptions,
) -> anyhow::Result<RunExecutable> {
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
    preconfig.try_tcc_run = options.try_tcc_run;

    let output = UserDiagnostics::from_flags(cli);
    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        &target_dir,
        output,
        &resolved,
    )?;
    let package = rr_build::local_packages(&resolved)
        .next()
        .expect("Single-file project must synthesize exactly one package");
    let directive = if value_tracing {
        rr_build::build_patch_directive_for_package(package, false, Some(package), None, false)?
    } else {
        Default::default()
    };
    let intent = (vec![UserIntent::Run(package)], directive).into();
    let (build_meta, build_graph) = rr_build::plan_resolved_build_from_intent(
        preconfig,
        &cli.unstable_feature,
        &target_dir,
        output,
        planning_context,
        intent,
        resolved,
    )?;

    build_executable_from_plan(
        cli,
        cmd,
        &source_dir,
        &target_dir,
        &build_meta,
        build_graph,
        BuildExecutableFromPlanOptions {
            force_success_exit: false,
            print_dry_run_run_command: options.print_dry_run_run_command,
        },
    )
}

#[instrument(level = Level::DEBUG, skip_all)]
/// Execute the build graph and return the resulting run artifact without
/// launching it.
fn build_executable_from_plan(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    build_meta: &rr_build::BuildMeta,
    build_graph: rr_build::BuildInput,
    options: BuildExecutableFromPlanOptions,
) -> Result<RunExecutable, anyhow::Error> {
    if cli.dry_run {
        rr_build::print_dry_run(
            &build_graph,
            build_meta.artifacts.values(),
            source_dir,
            target_dir,
        );

        if options.print_dry_run_run_command {
            let run_cmd = get_run_cmd(build_meta, &cmd.args);
            rr_build::dry_print_command(run_cmd.as_std(), source_dir, false);
        }
        return Ok(RunExecutable {
            executable: get_run_executable(build_meta).to_path_buf(),
            target_backend: build_meta.target_backend,
            opt_level: build_meta.opt_level,
            target_dir: target_dir.to_path_buf(),
            source_dir: source_dir.to_path_buf(),
            build_exit_code: None,
            force_success_exit: options.force_success_exit,
            lock: None,
        });
    }

    let lock = FileLock::lock(target_dir)?;
    // Generate all_pkgs.json for indirect dependency resolution
    rr_build::generate_all_pkgs_json(target_dir, build_meta, RunMode::Run)?;

    let build_config = BuildConfig::from_flags(
        &cmd.build_flags,
        &cli.unstable_feature,
        cli.verbose,
        UserDiagnostics::from_flags(cli),
    );
    let build_result = rr_build::execute_build(&build_config, build_graph, target_dir)?;

    Ok(RunExecutable {
        executable: get_run_executable(build_meta).to_path_buf(),
        target_backend: build_meta.target_backend,
        opt_level: build_meta.opt_level,
        target_dir: target_dir.to_path_buf(),
        source_dir: source_dir.to_path_buf(),
        build_exit_code: Some(build_result.return_code_for_success()),
        force_success_exit: options.force_success_exit,
        lock: Some(lock),
    })
}

#[instrument(level = Level::DEBUG, skip_all)]
/// Consume a built run artifact using normal `moon run` semantics.
///
/// This handles dry-run, build failure exit codes, `--build-only`, verbose
/// command printing, lock release, and finally process execution.
fn run_executable(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
    mut executable: RunExecutable,
) -> Result<i32, anyhow::Error> {
    if cli.dry_run {
        return Ok(0);
    }

    let build_exit_code = executable
        .build_exit_code
        .expect("non-dry run build should produce a build exit code");
    if build_exit_code != 0 {
        if executable.force_success_exit {
            return Ok(0);
        }
        return Ok(build_exit_code);
    }

    let mut run_cmd = crate::run::command_for(
        executable.target_backend,
        executable.executable.as_path(),
        None,
    );
    run_cmd.args(&cmd.args);
    if cli.verbose {
        rr_build::dry_print_command(run_cmd.as_std(), &executable.source_dir, true);
    }

    // Release the lock before spawning the subprocess
    executable.release_lock();

    if cmd.build_only {
        let test_artifacts = TestArtifacts {
            artifacts_path: vec![executable.executable],
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
        if executable.force_success_exit {
            Ok(0)
        } else {
            Ok(code)
        }
    } else {
        bail!("Command exited without a return code")
    }
}
