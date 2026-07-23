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
use mooncake::pkg::sync::SyncOutputOptions;
use moonutil::cli_support::AutoSyncFlags;
use moonutil::command_output::CommandOutput;
use moonutil::project::{PackageDirs, ProjectProbe};
use moonutil::{
    build_options::{RunMode, TestArtifacts},
    constants::is_moon_pkg_exist,
    locks::FileLock,
    target::TargetBackend,
    user_log::UserLog,
};
use tracing::{Level, instrument};

use crate::filter::ensure_package_supports_backend;
use crate::rr_build;
use crate::rr_build::preconfig_compile;
use crate::rr_build::{BuildConfig, CalcUserIntentOutput};
use crate::run::default_rt;

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

    #[clap(skip)]
    pub(crate) moonrun_policy: Option<PathBuf>,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Only build, do not run the code
    #[clap(long, conflicts_with = "profile")]
    pub build_only: bool,

    /// Profile the native executable using Time Profiler on macOS or perf on Linux
    #[clap(long)]
    pub profile: bool,
}

#[derive(Debug, Clone, Copy)]
struct RunOutputVerbosity {
    verbose: bool,
}

impl RunOutputVerbosity {
    fn from_flags(cli: &UniversalFlags) -> Self {
        Self {
            verbose: cli.verbose,
        }
    }

    fn sync_output(self) -> SyncOutputOptions {
        SyncOutputOptions::new(!self.verbose, self.verbose)
    }

    fn suppress_build_progress(self) -> bool {
        !self.verbose
    }
}

/// Controls how `moon run` builds the executable before it is consumed.
///
/// Normal execution preserves the existing debug-native fast path by allowing
/// `tcc -run`. Consumers that need a standalone executable, such as profilers,
/// should disable it.
pub(crate) struct BuildRunExecutableOptions {
    /// Whether native debug builds may use `tcc -run`.
    ///
    /// `tcc -run` executes through `tcc @rspfile` and does not provide the same
    /// standalone executable shape as regular native execution.
    try_tcc_run: bool,
    /// Whether dry-run output should include the final executable invocation.
    print_dry_run_run_command: bool,
    output: RunOutputVerbosity,
    /// Backend to use when neither CLI flags nor single-file metadata selects one.
    default_target_backend: TargetBackend,
}

impl BuildRunExecutableOptions {
    fn for_run(cli: &UniversalFlags) -> Self {
        Self {
            try_tcc_run: !cli.dry_run,
            print_dry_run_run_command: true,
            output: RunOutputVerbosity::from_flags(cli),
            default_target_backend: TargetBackend::default(),
        }
    }

    pub(crate) fn for_profile(cli: &UniversalFlags) -> Self {
        Self {
            // Profiling needs a stable executable path for xctrace to launch.
            // The TCC fast path may run directly from generated C instead.
            try_tcc_run: false,
            // The dry-run output should show the profiled invocation, not the
            // plain executable command that `moon run` would normally print.
            print_dry_run_run_command: false,
            output: RunOutputVerbosity::from_flags(cli),
            default_target_backend: TargetBackend::default(),
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
    /// Present only when native debug execution selected `tcc -run`.
    pub(crate) tcc_run: Option<moonbuild_rupes_recta::model::TccRunConfig>,
    pub(crate) opt_level: moonutil::cond_expr::OptLevel,
    pub(crate) target_dir: PathBuf,
    source_dir: PathBuf,
    build_exit_code: Option<i32>,
    lock: Option<FileLock>,
}

struct BuildExecutableFromPlanOptions {
    print_dry_run_run_command: bool,
    output: RunOutputVerbosity,
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
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let mut source = String::new();
    std::io::stdin()
        .read_to_string(&mut source)
        .context("failed to read `.mbtx` source from stdin")?;

    run_source_as_single_file(
        cli,
        cmd,
        source,
        "stdin.mbtx",
        "stdin",
        BuildRunExecutableOptions::for_run(cli),
        output,
    )
}

fn run_inline_source_as_single_file(
    cli: &UniversalFlags,
    cmd: RunSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let source = cmd
        .command
        .clone()
        .expect("inline script should be present when `moon run -e` is selected");

    run_source_as_single_file(
        cli,
        cmd,
        source,
        "command.mbtx",
        "command",
        BuildRunExecutableOptions::for_run(cli),
        output,
    )
}

fn run_source_as_single_file(
    cli: &UniversalFlags,
    cmd: RunSubcommand,
    source: String,
    temp_name: &str,
    source_name: &str,
    options: BuildRunExecutableOptions,
    output: &CommandOutput,
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
        moonrun_policy,
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
        moonrun_policy,
        auto_sync_flags,
        build_only,
        profile,
    };
    let result = run_single_file_from_arg_with_options(cli, cmd, options, output);
    drop(temp_dir);
    result
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
    output: &CommandOutput,
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
        let mut run_cmd = crate::run::command_for_with_moonrun_policy(
            moonbuild_rupes_recta::model::RunBackend::WasmGC,
            None,
            &wasm_path,
            None,
            cmd.moonrun_policy.as_deref(),
        );
        run_cmd.args(&cmd.args);
        let command = rr_build::format_dry_run_command(run_cmd.as_std(), &print_dir);
        output.write_result(|writer| writeln!(writer, "{command}"))?;
    }

    Ok(RunExecutable {
        executable: wasm_path,
        target_backend: moonbuild_rupes_recta::model::RunBackend::WasmGC,
        tcc_run: None,
        opt_level: moonutil::cond_expr::OptLevel::Debug,
        target_dir: print_dir.clone(),
        source_dir: print_dir,
        build_exit_code: (!cli.dry_run).then_some(0),
        lock: None,
    })
}

#[instrument(skip_all)]
pub(crate) fn run_run(
    cli: &UniversalFlags,
    cmd: RunSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    if cmd.profile {
        return super::profile::run_profiled_run(cli, cmd, output);
    }

    if cmd.command.is_some() {
        return run_inline_source_as_single_file(cli, cmd, output);
    }

    if cmd.package_or_mbt_file.as_deref() == Some("-") {
        return run_stdin_source_as_single_file(cli, cmd, output);
    }

    let executable = if cmd.package_or_mbt_file.as_deref().is_some_and(|input| {
        Path::new(input)
            .extension()
            .is_some_and(|ext| ext == "wasm")
            && std::fs::metadata(input).is_ok_and(|metadata| metadata.is_file())
    }) {
        build_wasm_file_executable_from_arg(cli, &cmd, output)?
    } else {
        build_run_executable(cli, &cmd, BuildRunExecutableOptions::for_run(cli), output)?
    };
    let result = run_executable(cli, &cmd, executable, output);
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
    output: &CommandOutput,
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
        return build_wasm_file_executable_from_arg(cli, cmd, output);
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
                return build_single_file_executable_from_arg(cli, cmd, options, output);
            }
            if is_mbt {
                let moon_pkg_json_exist =
                    std::fs::metadata(input)?.is_file() && is_moon_pkg_exist(&run_start_dir);
                if !moon_pkg_json_exist {
                    return build_single_file_executable_from_arg(cli, cmd, options, output);
                }
            }
        }
        ProjectProbe::NotFound(not_found) => {
            if is_mbt || is_mbtx {
                return build_single_file_executable_from_arg(cli, cmd, options, output);
            }
            return Err(not_found.into_error().into());
        }
    }

    let selected_target_backend = cmd.build_flags.resolve_single_target_backend()?;
    build_package_executable(cli, cmd, selected_target_backend, options, output)
}

#[instrument(skip_all)]
/// Build a package run target after the top-level input has been classified as
/// a package inside a MoonBit project.
fn build_package_executable(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
    selected_target_backend: Option<TargetBackend>,
    options: BuildRunExecutableOptions,
    output: &CommandOutput,
) -> Result<RunExecutable, anyhow::Error> {
    let user_log = output.user_log();
    let run_start_dir = resolve_run_start_dir(
        cmd.package_or_mbt_file
            .as_deref()
            .expect("package run planning requires a positional input"),
    )?;
    let dirs = cli
        .source_tgt_dir
        .query_from(&run_start_dir, cli.workspace_env.clone())?
        .package_dirs()?;
    let PackageDirs {
        source_dir,
        target_dir,
        mooncake_bin_dir,
        ..
    } = &dirs;

    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        !cmd.build_flags.std(),
        cmd.build_flags.enable_coverage,
        cli.workspace_env.clone(),
    )
    .with_sync_output(options.output.sync_output());
    let synced_env = moonbuild_rupes_recta::sync_dependencies(&resolve_cfg, &dirs)?;
    let resolve_output =
        moonbuild_rupes_recta::resolve_synced_project(&resolve_cfg, synced_env, user_log)?;
    let (build_meta, build_graph) = plan_run_rr_from_resolved(
        cli,
        cmd,
        target_dir,
        mooncake_bin_dir,
        selected_target_backend,
        resolve_output,
        options.try_tcc_run,
        user_log,
    )?;
    build_executable_from_plan(
        cli,
        cmd,
        source_dir,
        target_dir,
        &build_meta,
        build_graph,
        BuildExecutableFromPlanOptions {
            print_dry_run_run_command: options.print_dry_run_run_command,
            output: options.output,
        },
        output,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_run_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    resolve_output: ResolveOutput,
    try_tcc_run: bool,
    user_log: &UserLog,
) -> anyhow::Result<(rr_build::BuildMeta, rr_build::BuildInput)> {
    let input_path = cmd
        .package_or_mbt_file
        .clone()
        .expect("package run planning requires a positional input");
    let selection = resolve_run_selection(&input_path, &resolve_output)?;
    let package = resolve_output.pkg_dirs.get_package(selection.package);
    let selected_target_backend = Some(
        selected_target_backend
            .or_else(|| {
                resolve_output
                    .module_rel
                    .module_info(package.module)
                    .preferred_target
            })
            .unwrap_or_default(),
    );
    let mut preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        target_dir,
        RunMode::Run,
    );
    preconfig.try_tcc_run = try_tcc_run;

    let value_tracing = cmd.build_flags.enable_value_tracing;

    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        user_log,
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
        user_log,
        planning_context,
        intent,
        mooncake_bin_dir,
        resolve_output,
    )
}

#[instrument(level = Level::DEBUG, skip_all)]
fn get_run_cmd(
    build_meta: &rr_build::BuildMeta,
    argv: &[String],
    moonrun_policy: Option<&Path>,
) -> tokio::process::Command {
    let executable = get_run_executable(build_meta);
    let mut cmd = crate::run::command_for_with_moonrun_policy(
        build_meta.target_backend,
        build_meta.tcc_run.as_ref(),
        executable,
        None,
        moonrun_policy,
    );
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
fn run_single_file_from_arg_with_options(
    cli: &UniversalFlags,
    cmd: RunSubcommand,
    options: BuildRunExecutableOptions,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let executable = build_single_file_executable_from_arg(cli, &cmd, options, output)?;
    run_executable(cli, &cmd, executable, output)
}

/// Build a standalone `.mbt`/`.mbtx` input through the synthesized single-file
/// package machinery.
fn build_single_file_executable_from_arg(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
    options: BuildRunExecutableOptions,
    output: &CommandOutput,
) -> anyhow::Result<RunExecutable> {
    let input = cmd
        .package_or_mbt_file
        .as_deref()
        .expect("single-file run from arg requires a positional input path");
    let single_file_dirs = if Path::new(input)
        .extension()
        .is_some_and(|extension| extension == "mbtx")
    {
        cli.source_tgt_dir.cached_single_file_package_dirs(input)?
    } else {
        cli.source_tgt_dir.single_file_package_dirs(input)?
    };
    build_single_file_executable(
        cli,
        cmd,
        single_file_dirs.package_dirs,
        single_file_dirs.file_path,
        options,
        output,
    )
}

#[instrument(level = Level::DEBUG, skip_all)]
/// Build a run executable from already-resolved single-file package directories.
#[allow(clippy::too_many_arguments)]
fn build_single_file_executable(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
    dirs: PackageDirs,
    input_path: std::path::PathBuf,
    options: BuildRunExecutableOptions,
    output: &CommandOutput,
) -> anyhow::Result<RunExecutable> {
    let user_log = output.user_log();
    let PackageDirs {
        source_dir,
        target_dir,
        mooncake_bin_dir,
        ..
    } = &dirs;
    std::fs::create_dir_all(target_dir).context("failed to create target directory")?;

    let value_tracing = cmd.build_flags.enable_value_tracing;

    let selected_target_backend = cmd.build_flags.resolve_single_target_backend()?;

    // Resolve single-file project (synthesized package around the file)
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        false,
        cmd.build_flags.enable_coverage,
        cli.workspace_env.clone(),
    )
    .with_sync_output(options.output.sync_output());
    let (resolved, backend) = moonbuild_rupes_recta::resolve::resolve_single_file_project(
        &resolve_cfg,
        &dirs,
        &input_path,
        true,
        user_log,
    )?;
    let selected_target_backend = selected_target_backend
        .or(backend)
        .unwrap_or(options.default_target_backend);

    let mut preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        Some(selected_target_backend),
        target_dir,
        RunMode::Run,
    );
    preconfig.try_tcc_run = options.try_tcc_run;

    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        user_log,
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
        user_log,
        planning_context,
        intent,
        mooncake_bin_dir,
        resolved,
    )?;

    build_executable_from_plan(
        cli,
        cmd,
        source_dir,
        target_dir,
        &build_meta,
        build_graph,
        BuildExecutableFromPlanOptions {
            print_dry_run_run_command: options.print_dry_run_run_command,
            output: options.output,
        },
        output,
    )
}

#[instrument(level = Level::DEBUG, skip_all)]
/// Execute the build graph and return the resulting run artifact without
/// launching it.
#[allow(clippy::too_many_arguments)]
fn build_executable_from_plan(
    cli: &UniversalFlags,
    cmd: &RunSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    build_meta: &rr_build::BuildMeta,
    build_graph: rr_build::BuildInput,
    options: BuildExecutableFromPlanOptions,
    output: &CommandOutput,
) -> Result<RunExecutable, anyhow::Error> {
    let user_log = output.user_log();
    if cli.dry_run {
        output.write_result(|writer| {
            rr_build::write_dry_run(
                writer,
                &build_graph,
                build_meta.artifacts.values(),
                source_dir,
                target_dir,
            )?;

            if options.print_dry_run_run_command {
                let run_cmd = get_run_cmd(build_meta, &cmd.args, cmd.moonrun_policy.as_deref());
                writeln!(
                    writer,
                    "{}",
                    rr_build::format_dry_run_command(run_cmd.as_std(), source_dir)
                )?;
            }
            Ok::<_, std::io::Error>(())
        })?;
        return Ok(RunExecutable {
            executable: get_run_executable(build_meta).to_path_buf(),
            target_backend: build_meta.target_backend,
            tcc_run: build_meta.tcc_run.clone(),
            opt_level: build_meta.opt_level,
            target_dir: target_dir.to_path_buf(),
            source_dir: source_dir.to_path_buf(),
            build_exit_code: None,
            lock: None,
        });
    }

    let lock = FileLock::lock_with_verbosity(target_dir, options.output.verbose)?;
    // Generate all_pkgs.json for indirect dependency resolution
    rr_build::generate_all_pkgs_json(build_meta)?;

    let build_config =
        BuildConfig::from_flags(&cmd.build_flags, &cli.unstable_feature, cli.verbose)
            .with_suppressed_progress(options.output.suppress_build_progress());
    let build_result = rr_build::execute_build(&build_config, build_graph, target_dir, user_log)?;

    Ok(RunExecutable {
        executable: get_run_executable(build_meta).to_path_buf(),
        target_backend: build_meta.target_backend,
        tcc_run: build_meta.tcc_run.clone(),
        opt_level: build_meta.opt_level,
        target_dir: target_dir.to_path_buf(),
        source_dir: source_dir.to_path_buf(),
        build_exit_code: Some(build_result.return_code_for_success()),
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
    output: &CommandOutput,
) -> Result<i32, anyhow::Error> {
    if cli.dry_run {
        return Ok(0);
    }

    let build_exit_code = executable
        .build_exit_code
        .expect("non-dry run build should produce a build exit code");
    if build_exit_code != 0 {
        return Ok(build_exit_code);
    }

    let mut run_cmd = crate::run::command_for_with_moonrun_policy(
        executable.target_backend,
        executable.tcc_run.as_ref(),
        executable.executable.as_path(),
        None,
        cmd.moonrun_policy.as_deref(),
    );
    run_cmd.args(&cmd.args);
    output.user_log().info(rr_build::format_dry_run_command(
        run_cmd.as_std(),
        &executable.source_dir,
    ));

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
        Ok(code)
    } else {
        bail!("Command exited without a return code")
    }
}
