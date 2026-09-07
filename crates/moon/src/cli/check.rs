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

//! Workflow overview for `moon check`:
//!
//! 1. Parse the CLI selector (`PATH`, `-p`, or whole workspace).
//! 2. Resolve packages and decide which backend(s) to check:
//!    - explicit `--target` keeps one backend;
//!    - otherwise local packages are grouped by
//!      `module preferred -> workspace preferred -> default backend`.
//! 3. `plan_check_rr_from_resolved_all` turns those backend groups into an
//!    ordered list of single-backend RR plans.
//! 4. `plan_check_rr_from_resolved` still plans exactly one backend group.
//! 5. The command layer composes those plans into one executor graph. Project
//!    checks without a selector publish `packages.json` and `index.json`;
//!    focused checks leave
//!    it untouched.
//!
use anyhow::Context;
use log::LevelFilter;
use moonbuild_rupes_recta::intent::UserIntent;
use moonbuild_rupes_recta::model::PackageId;
use moonutil::build_options::RunMode;
use moonutil::child_process::ChildOutputMode;
use moonutil::cli_support::AutoSyncFlags;
use moonutil::cli_support::UniversalFlags;
use moonutil::command_output::CommandOutput;
use moonutil::constants::WATCH_MODE_DIR;
use moonutil::locks::lock_directory;
use moonutil::project::{PackageDirs, ProjectProbe};
use moonutil::target::TargetBackend;
use moonutil::target::lower_surface_targets;
use moonutil::user_log::{UserLog, UserLogCapture, UserLogEntry, UserLogEntryLevel};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tracing::{Level, instrument};

use crate::filter::{
    TargetPackageGroup, canonicalize_with_filename, ensure_package_supports_backend,
    ensure_packages_support_backend, filter_pkg_by_dir, format_supported_backends,
    group_packages_by_preferred_backend, package_supports_backend, select_packages,
    select_supported_packages,
};
use crate::rr_build::{self, BuildConfig, CalcUserIntentOutput, preconfig_compile};
use crate::watch::prebuild_output::{PrebuildWatchPaths, rr_get_prebuild_watch_paths};
use crate::watch::{WatchOutput, watching};

use super::BuildFlags;
use super::invocation::{JsonCommand, JsonCommandOutcome};

const CHECK_JSON_ERROR_EXIT_CODE: i32 = -1;

#[derive(Debug, Clone)]
struct ResolvedCheckSelection {
    packages: Vec<PackageId>,
    patch_file: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct MoonMessage {
    #[serde(rename = "$message_type")]
    message_type: &'static str,
    level: UserLogEntryLevel,
    message: String,
}

impl From<UserLogEntry> for MoonMessage {
    fn from(entry: UserLogEntry) -> Self {
        Self {
            message_type: "moon",
            level: entry.level,
            message: entry.message,
        }
    }
}

#[derive(Debug, Default, Serialize)]
struct CheckJsonSummary {
    tasks_executed: Option<usize>,
    moon_errors: usize,
    moon_warnings: usize,
    diagnostic_errors: usize,
    diagnostic_warnings: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    hidden_diagnostic_errors: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hidden_diagnostic_warnings: Option<usize>,
}

#[derive(Debug, Serialize)]
struct CheckJsonReport {
    version: u32,
    status: &'static str,
    diagnostics: Vec<serde_json::Value>,
    messages: Vec<MoonMessage>,
    summary: CheckJsonSummary,
}

#[derive(Debug, Default)]
struct CheckJsonAccumulator {
    diagnostics: Vec<serde_json::Value>,
    messages: Vec<MoonMessage>,
    tasks_executed: Option<usize>,
    build_failed: bool,
    diagnostic_errors: usize,
    diagnostic_warnings: usize,
    hidden_diagnostic_errors: usize,
    hidden_diagnostic_warnings: usize,
}

impl CheckJsonAccumulator {
    fn append_build(&mut self, result: rr_build::JsonBuildOutput, user_log: &UserLog) {
        let successful = result.successful();
        self.diagnostics
            .extend(result.diagnostics.into_iter().map(|diagnostic| {
                let mut value = diagnostic.value;
                if let Some(target_backend) = diagnostic.target_backend {
                    value
                        .as_object_mut()
                        .expect("Moonc diagnostic should be a JSON object")
                        .insert(
                            "target_backend".to_string(),
                            serde_json::Value::String(target_backend.to_flag().to_string()),
                        );
                }
                value
            }));
        self.diagnostic_errors += result.n_errors;
        self.diagnostic_warnings += result.n_warnings;
        self.hidden_diagnostic_errors += result.hidden_errors;
        self.hidden_diagnostic_warnings += result.hidden_warnings;
        self.tasks_executed = if successful && !self.build_failed {
            Some(self.tasks_executed.unwrap_or_default() + result.n_tasks_executed.unwrap())
        } else {
            None
        };
        self.build_failed |= !successful;

        for message in result.non_diagnostic_output {
            if successful {
                user_log.info(message);
            } else {
                user_log.error(message);
            }
        }
        if result.hidden_errors != 0 || result.hidden_warnings != 0 {
            user_log.warn(format!(
                    "diagnostic output limited by --diagnostic-limit: {} errors and {} warnings were not displayed.",
                    result.hidden_errors, result.hidden_warnings
                ));
        }
    }

    fn error(&mut self, error: impl std::fmt::Display) {
        self.messages.push(MoonMessage {
            message_type: "moon",
            level: UserLogEntryLevel::Error,
            message: error.to_string(),
        });
    }
}

struct CheckJsonOutcome {
    exit_code: i32,
    accumulator: CheckJsonAccumulator,
}

impl CheckJsonOutcome {
    fn from_error(exit_code: i32, error: impl std::fmt::Display) -> Self {
        let mut accumulator = CheckJsonAccumulator::default();
        accumulator.error(error);
        Self {
            exit_code,
            accumulator,
        }
    }

    fn exit_code(&self) -> i32 {
        self.exit_code
    }
}

#[derive(Debug)]
struct CheckJsonCommand {
    command: CheckSubcommand,
}

pub(crate) fn json_command(command: CheckSubcommand) -> Box<dyn JsonCommand> {
    Box::new(CheckJsonCommand { command })
}

impl JsonCommand for CheckJsonCommand {
    fn run(&self, flags: &UniversalFlags, output: &CommandOutput) -> JsonCommandOutcome {
        check_json_outcome(run_check_json(flags, &self.command, output))
    }

    fn bootstrap_error(&self, message: String) -> JsonCommandOutcome {
        check_json_outcome(CheckJsonOutcome::from_error(
            CHECK_JSON_ERROR_EXIT_CODE,
            message,
        ))
    }
}

fn check_json_outcome(outcome: CheckJsonOutcome) -> JsonCommandOutcome {
    let exit_code = outcome.exit_code();
    JsonCommandOutcome::new(exit_code, move |output, capture| {
        write_check_json(output, capture, outcome)
    })
}

impl ResolvedCheckSelection {
    fn from_command(packages: Vec<PackageId>, cmd: &CheckSubcommand) -> Self {
        Self {
            packages,
            patch_file: cmd.patch_file.clone(),
        }
    }

    fn into_user_intent(self) -> anyhow::Result<CalcUserIntentOutput> {
        let directive =
            build_directive_for_selected_packages(&self.packages, self.patch_file.as_deref())?;
        Ok((
            self.packages.into_iter().map(UserIntent::Check).collect(),
            directive,
        )
            .into())
    }
}

/// Check the current package, but don't build object files
#[derive(Debug, clap::Parser, Clone)]
#[clap(group = clap::ArgGroup::new("package_selector").multiple(false))]
pub(crate) struct CheckSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Monitor the file system and automatically check files
    #[clap(long, short)]
    pub watch: bool,

    /// Legacy package directory path relative to the module source root (`source` in `moon.mod.json`)
    //
    // This selects a package directory under the module source root, not an arbitrary
    // filesystem path. Use positional `PATH` for filesystem paths.
    // TODO: Unify the `-p` flag to specifying package name, see #1139
    #[clap(
        long,
        short_alias = 'p',
        value_name = "PACKAGE_DIR",
        hide = true,
        group = "package_selector"
    )]
    pub package_path: Option<PathBuf>,

    /// The patch file to check. Only valid when the selector resolves to a single package.
    #[clap(long, requires = "package_selector")]
    pub patch_file: Option<PathBuf>,

    /// Whether to explain the error code with details.
    #[clap(long)]
    pub explain: bool,

    /// Filesystem path to a package directory, `.mbt` / `.mbt.md` selector, or standalone `.mbtx` file
    #[clap(conflicts_with = "watch", name = "PATH", group = "package_selector")]
    pub path: Vec<PathBuf>,

    /// Check whether the code is properly formatted
    #[clap(long)]
    pub fmt: bool,

    /// Output one complete JSON result to stdout
    #[clap(long)]
    pub json: bool,
}

fn write_check_json(
    output: &CommandOutput,
    capture: &UserLogCapture,
    mut outcome: CheckJsonOutcome,
) -> anyhow::Result<()> {
    let mut captured_messages = capture
        .take()
        .into_iter()
        .map(MoonMessage::from)
        .collect::<Vec<_>>();
    captured_messages.append(&mut outcome.accumulator.messages);
    let moon_errors = captured_messages
        .iter()
        .filter(|message| matches!(message.level, UserLogEntryLevel::Error))
        .count();
    let moon_warnings = captured_messages
        .iter()
        .filter(|message| matches!(message.level, UserLogEntryLevel::Warning))
        .count();
    let report = CheckJsonReport {
        version: 1,
        status: if outcome.exit_code == 0 {
            "success"
        } else {
            "failure"
        },
        diagnostics: outcome.accumulator.diagnostics,
        messages: captured_messages,
        summary: CheckJsonSummary {
            tasks_executed: outcome.accumulator.tasks_executed,
            moon_errors,
            moon_warnings,
            diagnostic_errors: outcome.accumulator.diagnostic_errors,
            diagnostic_warnings: outcome.accumulator.diagnostic_warnings,
            hidden_diagnostic_errors: (outcome.accumulator.hidden_diagnostic_errors != 0)
                .then_some(outcome.accumulator.hidden_diagnostic_errors),
            hidden_diagnostic_warnings: (outcome.accumulator.hidden_diagnostic_warnings != 0)
                .then_some(outcome.accumulator.hidden_diagnostic_warnings),
        },
    };
    output.write_result(|writer| -> anyhow::Result<()> {
        serde_json::to_writer(&mut *writer, &report)?;
        writeln!(writer)?;
        Ok(())
    })
}

fn run_check_json(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    output: &CommandOutput,
) -> CheckJsonOutcome {
    let incompatible = [
        (cmd.watch, "--watch"),
        (cli.dry_run, "--dry-run"),
        (cmd.fmt, "--fmt"),
        (cmd.build_flags.no_render, "--no-render"),
        (cmd.build_flags.output_json, "--output-json"),
    ]
    .into_iter()
    .filter_map(|(enabled, flag)| enabled.then_some(flag))
    .collect::<Vec<_>>();
    if !incompatible.is_empty() {
        return CheckJsonOutcome::from_error(
            2,
            format!("--json cannot be used with {}", incompatible.join(", ")),
        );
    }

    let mut json_cmd = cmd.clone();
    json_cmd.build_flags.output_json = true;
    let mut accumulator = CheckJsonAccumulator::default();
    match run_check_impl(cli, &json_cmd, output, Some(&mut accumulator)) {
        Ok(exit_code) => CheckJsonOutcome {
            exit_code,
            accumulator,
        },
        Err(error) => {
            accumulator.error(format!("{error:#}"));
            CheckJsonOutcome {
                exit_code: CHECK_JSON_ERROR_EXIT_CODE,
                accumulator,
            }
        }
    }
}

#[instrument(skip_all)]
pub(crate) fn run_check(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    run_check_impl(cli, cmd, output, None)
}

fn run_check_impl(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    output: &CommandOutput,
    mut json: Option<&mut CheckJsonAccumulator>,
) -> anyhow::Result<i32> {
    let user_log = output.user_log();
    if cmd.fmt {
        let mut cli_for_fmt = cli.clone();
        cli_for_fmt.quiet = true;
        let fmt_output = CommandOutput::new(LevelFilter::Error);
        let fmt_exit_code = crate::cli::fmt::run_fmt(
            &cli_for_fmt,
            crate::cli::FmtSubcommand {
                check: false,
                sort_input: false,
                warn: true,
                path: cmd.path.clone(),
                args: vec![],
            },
            &fmt_output,
        )?;
        if fmt_exit_code != 0 {
            user_log.warn("formatting code failed");
        }
    }

    let (mut dirs, single_file) = if let Some(path) =
        super::standalone_mbtx_path(&cmd.path, "moon check")?
    {
        let single_file = cli.source_tgt_dir.single_file_package_dirs(path)?;
        (single_file.package_dirs, Some(single_file.file_path))
    } else {
        // Check if we're running within a project.
        let query = cli.source_tgt_dir.query(cli.workspace_env.clone())?;
        match query.probe_project()? {
            ProjectProbe::Found(_) => {
                let dirs = query.select(user_log)?.package_dirs()?;
                (dirs, None)
            }
            ProjectProbe::NotFound(not_found) => match cmd.path.as_slice() {
                [path] => {
                    let single_file = cli.source_tgt_dir.single_file_package_dirs(path)?;
                    (single_file.package_dirs, Some(single_file.file_path))
                }
                [] => return Err(not_found.into_error().into()),
                _ => {
                    anyhow::bail!("standalone single-file `moon check` expects exactly one `PATH`");
                }
            },
        }
    };
    let watch_ignored_subtree = dirs.target_dir.clone();
    if cmd.watch {
        dirs.target_dir = dirs.target_dir.join(WATCH_MODE_DIR);
        dirs.mooncake_bin_dir = dirs.target_dir.join(moonutil::constants::MOON_BIN_DIR);
    }

    let targets = if cmd.build_flags.target.is_empty() {
        Vec::new()
    } else {
        lower_surface_targets(&cmd.build_flags.target)
    };

    // Standalone inputs require their own resolution front end, but their
    // target plans converge with project checks in `run_planned_checks`.
    // Standalone Check is currently one-shot; only project checks enter the
    // watcher below.
    if let Some(single_file_path) = single_file.as_deref() {
        let result =
            run_check_for_single_file_rr(cli, cmd, single_file_path, &dirs, &targets, output, json);
        return match targets.as_slice() {
            [] => result,
            [target] => result.context(format!("failed to run check for target {target:?}")),
            _ => result.context(format!("failed to run check for targets {targets:?}")),
        };
    }

    if targets.is_empty() {
        return run_check_normal_internal(
            cli,
            cmd,
            &dirs,
            &watch_ignored_subtree,
            None,
            output,
            json,
        );
    }

    if cmd.watch {
        let mut ret_value = 0;
        for t in targets {
            let x = run_check_normal_internal(
                cli,
                cmd,
                &dirs,
                &watch_ignored_subtree,
                Some(t),
                output,
                json.as_deref_mut(),
            )
            .context(format!("failed to run check for target {t:?}"))?;
            ret_value = ret_value.max(x);
        }
        return Ok(ret_value);
    }

    let resolve_output =
        sync_and_resolve_check_project(cli, cmd, &dirs, output.user_log(), json.is_some())
            .context("Failed to calculate build plan")?;
    let _lock;
    if !cli.dry_run {
        _lock = lock_directory(&dirs.target_dir, output.user_log()).with_context(|| {
            format!(
                "failed to acquire build lock in target directory `{}`",
                dirs.target_dir.display()
            )
        })?;
    }
    let result = run_check_normal_rr_from_resolved(
        cli,
        cmd,
        &dirs,
        false,
        &targets,
        resolve_output,
        output,
        json,
    )
    .with_context(|| match targets.as_slice() {
        [target] => format!("failed to run check for target {target:?}"),
        _ => format!("failed to run check for targets {targets:?}"),
    })?;
    Ok(if result.ok { 0 } else { 1 })
}

/// Resolves a standalone input once and plans every selected Target Backend.
/// Finalization and execution are shared with project checks.
#[allow(clippy::too_many_arguments)]
fn run_check_for_single_file_rr(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    single_file_path: &Path,
    dirs: &PackageDirs,
    selected_target_backends: &[TargetBackend],
    output: &CommandOutput,
    json: Option<&mut CheckJsonAccumulator>,
) -> anyhow::Result<i32> {
    let user_log = output.user_log();
    let PackageDirs {
        target_dir,
        mooncake_bin_dir,
        ..
    } = dirs;
    if cmd.patch_file.is_some() {
        anyhow::bail!("standalone single-file `moon check` does not support `--patch-file`");
    }

    std::fs::create_dir_all(target_dir).context("failed to create target directory")?;

    // Manually synthesize and resolve single file project
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        false,
        cmd.build_flags.enable_coverage,
        cli.workspace_env.clone(),
    )
    .with_sync_output(mooncake::pkg::sync::SyncOutputOptions {
        quiet: false,
        child_output: if json.is_some() {
            ChildOutputMode::Capture
        } else {
            ChildOutputMode::Inherit
        },
    });
    let (resolved, backend) = moonbuild_rupes_recta::resolve::resolve_single_file_project(
        &resolve_cfg,
        dirs,
        single_file_path,
        false,
        user_log,
    )?;
    let target_backends = if selected_target_backends.is_empty() {
        vec![cmd.build_flags.resolve_single_target_backend()?.or(backend)]
    } else {
        selected_target_backends.iter().copied().map(Some).collect()
    };

    let _lock;
    if !cli.dry_run {
        _lock = lock_directory(target_dir, user_log).with_context(|| {
            format!(
                "failed to acquire build lock in target directory `{}`",
                target_dir.display()
            )
        })?;
    }

    let mut planned_runs = Vec::with_capacity(target_backends.len());
    for target_backend in target_backends {
        let preconfig = preconfig_compile(
            &cmd.auto_sync_flags,
            cli,
            &cmd.build_flags,
            target_backend,
            target_dir,
            RunMode::Check,
        );
        let planning_context = rr_build::prepare_resolved_build(
            &preconfig,
            &cli.unstable_feature,
            target_dir,
            user_log,
            &resolved,
        )?;
        let intent = get_user_intents_single_file(&resolved, planning_context.target_backend())?;
        planned_runs.push(
            rr_build::plan_resolved_build_from_intent(
                preconfig,
                &cli.unstable_feature,
                user_log,
                planning_context,
                intent,
                mooncake_bin_dir,
                resolved.clone(),
            )
            .context("Failed to calculate build plan")?,
        );
    }

    run_planned_checks(cli, cmd, dirs, planned_runs, true, output, json)
        .map(|ok| if ok { 0 } else { 1 })
}

fn get_user_intents_single_file(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    _backend: TargetBackend,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let m_packages = resolve_output
        .pkg_dirs
        .packages_for_module(resolve_output.local_modules()[0])
        .context("single-file project must resolve a local module")?;
    let pkg = *m_packages
        .iter()
        .next()
        .context("single-file project must resolve exactly one package")?
        .1;

    Ok(vec![UserIntent::Check(pkg)].into())
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
fn run_check_normal_internal(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    dirs: &PackageDirs,
    watch_ignored_subtree: &Path,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
    json: Option<&mut CheckJsonAccumulator>,
) -> anyhow::Result<i32> {
    if let Some(json) = json {
        return run_check_normal_internal_rr(
            cli,
            cmd,
            dirs,
            false,
            selected_target_backend,
            output,
            Some(json),
        )
        .map(|output| if output.ok { 0 } else { 1 });
    }
    let run_once = || -> anyhow::Result<WatchOutput> {
        run_check_normal_internal_rr(
            cli,
            cmd,
            dirs,
            cmd.watch,
            selected_target_backend,
            output,
            None,
        )
    };
    if cmd.watch {
        watching(run_once, &dirs.source_dir, watch_ignored_subtree)
    } else {
        run_once().map(|output| if output.ok { 0 } else { 1 })
    }
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
fn run_check_normal_internal_rr(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    dirs: &PackageDirs,
    watch: bool,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
    json: Option<&mut CheckJsonAccumulator>,
) -> anyhow::Result<WatchOutput> {
    let user_log = output.user_log();
    let resolve_output = sync_and_resolve_check_project(cli, cmd, dirs, user_log, json.is_some())
        .context("Failed to calculate build plan")?;
    let _lock;
    if !cli.dry_run {
        _lock = lock_directory(&dirs.target_dir, user_log).with_context(|| {
            format!(
                "failed to acquire build lock in target directory `{}`",
                dirs.target_dir.display()
            )
        })?;
    }
    run_check_normal_rr_from_resolved(
        cli,
        cmd,
        dirs,
        watch,
        selected_target_backend.as_slice(),
        resolve_output,
        output,
        json,
    )
}

/// Plans and executes a check from resolved project data.
///
/// The caller must hold the target-directory lock for a non-dry-run check.
#[allow(clippy::too_many_arguments)]
fn run_check_normal_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    dirs: &PackageDirs,
    watch: bool,
    selected_target_backends: &[TargetBackend],
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    output: &CommandOutput,
    json: Option<&mut CheckJsonAccumulator>,
) -> anyhow::Result<WatchOutput> {
    let user_log = output.user_log();
    let PackageDirs {
        source_dir,
        target_dir,
        mooncake_bin_dir,
        ..
    } = dirs;
    let prebuild_list = if watch {
        rr_get_prebuild_watch_paths(&resolve_output)
    } else {
        PrebuildWatchPaths {
            ignored_paths: Vec::new(),
            watched_paths: Vec::new(),
        }
    };
    let planned_runs = if selected_target_backends.is_empty() {
        plan_check_rr_from_resolved_all(
            cli,
            cmd,
            source_dir,
            target_dir,
            mooncake_bin_dir,
            None,
            resolve_output,
            user_log,
        )
        .context("Failed to calculate build plan")?
    } else {
        selected_target_backends
            .iter()
            .copied()
            .map(|target| {
                plan_check_rr_from_resolved_all(
                    cli,
                    cmd,
                    source_dir,
                    target_dir,
                    mooncake_bin_dir,
                    Some(target),
                    resolve_output.clone(),
                    user_log,
                )
            })
            .collect::<anyhow::Result<Vec<_>>>()
            .context("Failed to calculate build plan")?
            .into_iter()
            .flatten()
            .collect()
    };

    let ok = run_planned_checks(
        cli,
        cmd,
        dirs,
        planned_runs,
        cmd.package_path.is_none() && cmd.path.is_empty(),
        output,
        json,
    )?;

    Ok(WatchOutput {
        ok,
        additional_ignored_paths: prebuild_list.ignored_paths,
        additional_watched_paths: prebuild_list.watched_paths,
    })
}

/// Finalizes metadata and executes checks planned by either resolution front
/// end.
///
/// The caller must hold the target-directory lock for a non-dry-run check.
fn run_planned_checks(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    dirs: &PackageDirs,
    planned_runs: Vec<(rr_build::BuildMeta, rr_build::BuildInput)>,
    publish_metadata: bool,
    output: &CommandOutput,
    json: Option<&mut CheckJsonAccumulator>,
) -> anyhow::Result<bool> {
    if planned_runs.is_empty() {
        return Ok(true);
    }

    let PackageDirs {
        source_dir,
        target_dir,
        ..
    } = dirs;
    if cli.dry_run {
        output.write_result(|writer| {
            let (build_metas, build_inputs): (Vec<_>, Vec<_>) = planned_runs.into_iter().unzip();
            let build_input =
                rr_build::compose_build_inputs(build_inputs).map_err(std::io::Error::other)?;
            rr_build::write_dry_run(
                writer,
                &build_input,
                build_metas.iter().flat_map(|meta| meta.artifacts.values()),
                source_dir,
                target_dir,
            )?;
            Ok::<_, std::io::Error>(())
        })?;
        return Ok(true);
    }

    let mut cfg = BuildConfig::from_flags(
        &cmd.build_flags,
        &cli.unstable_feature,
        cli.verbose && json.is_none(),
    );
    cfg.patch_file = cmd.patch_file.clone();
    cfg.explain_errors |= cmd.explain;
    for (build_meta, build_input) in &planned_runs {
        // Generate all_pkgs.json for indirect dependency resolution
        rr_build::generate_all_pkgs_json(build_meta)?;
        if publish_metadata {
            rr_build::generate_metadata(source_dir, build_meta, build_input)?;
        }
    }
    if publish_metadata {
        // The compiler's universal format selects one active projection.
        // Before the split, repeated publication made the final planned
        // backend authoritative, so retain that deterministic behavior.
        let selected = &planned_runs
            .last()
            .expect("non-empty planned runs were checked above")
            .0;
        rr_build::generate_metadata_selector(selected)?;
        rr_build::generate_metadata_index(planned_runs.iter().map(|(build_meta, _)| build_meta))?;
    }

    let build_inputs = planned_runs.into_iter().map(|(_, input)| input).collect();
    let build_input = rr_build::compose_build_inputs(build_inputs)?;
    if let Some(json) = json {
        let result = rr_build::execute_build_json(
            &cfg.with_suppressed_progress(true),
            build_input,
            target_dir,
        )?;
        let successful = result.successful();
        json.append_build(result, output.user_log());
        Ok(successful)
    } else {
        let result = rr_build::execute_build(&cfg, build_input, target_dir, output.user_log())?;
        result.print_info(cli.quiet, "checking")?;
        Ok(result.successful())
    }
}

fn sync_and_resolve_check_project(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    dirs: &PackageDirs,
    user_log: &UserLog,
    json: bool,
) -> anyhow::Result<moonbuild_rupes_recta::ResolveOutput> {
    let resolve_config = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        !cmd.build_flags.std(),
        cmd.build_flags.enable_coverage,
        cli.workspace_env.clone(),
    )
    .with_sync_output(mooncake::pkg::sync::SyncOutputOptions {
        quiet: false,
        child_output: if json {
            ChildOutputMode::Capture
        } else {
            ChildOutputMode::Inherit
        },
    });
    rr_build::sync_and_resolve_project(&resolve_config, dirs, user_log)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_check_rr_from_resolved_all(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    user_log: &UserLog,
) -> anyhow::Result<Vec<(rr_build::BuildMeta, rr_build::BuildInput)>> {
    validate_selector_flags_before_split(
        &resolve_output,
        cmd,
        source_dir,
        selected_target_backend,
        user_log,
    )?;

    let selections = resolve_check_target_selections(
        &resolve_output,
        cmd,
        source_dir,
        selected_target_backend,
        user_log,
    )?;

    if selections.is_empty() {
        return plan_check_rr_from_resolved(
            cli,
            cmd,
            source_dir,
            target_dir,
            mooncake_bin_dir,
            selected_target_backend,
            resolve_output,
            user_log,
        )
        .map(|plan| vec![plan]);
    }

    selections
        .into_iter()
        .map(|selection| {
            // The command adapter has resolved raw CLI selectors into
            // PackageIds. RR planning should use those identities and the
            // bin-dependency launcher directory captured by the command
            // adapter.
            plan_check_rr_from_selection(
                cli,
                cmd,
                target_dir,
                mooncake_bin_dir,
                selection.target_backend,
                resolve_output.clone(),
                ResolvedCheckSelection::from_command(selection.packages, cmd),
                user_log,
            )
        })
        .collect()
}

fn validate_selector_flags_before_split(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_backend: Option<TargetBackend>,
    user_log: &UserLog,
) -> anyhow::Result<()> {
    if cmd.patch_file.is_none() {
        return Ok(());
    }

    let selected =
        resolve_selected_packages(resolve_output, cmd, source_dir, target_backend, user_log)?;
    if cmd.patch_file.is_some() && selected.len() != 1 {
        anyhow::bail!("`--patch-file` requires the selector to resolve to a single package");
    }
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_check_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    user_log: &UserLog,
) -> anyhow::Result<(rr_build::BuildMeta, rr_build::BuildInput)> {
    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        target_dir,
        RunMode::Check,
    );

    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        user_log,
        &resolve_output,
    )?;
    let intent = if let Some(filter_path) = cmd.package_path.as_deref() {
        calc_user_intent_from_package_path(
            &resolve_output,
            source_dir,
            filter_path,
            planning_context.target_backend(),
            cmd.patch_file.as_deref(),
        )?
    } else {
        calc_user_intent(
            &resolve_output,
            &cmd.path,
            planning_context.target_backend(),
            cmd.patch_file.as_deref(),
            user_log,
        )?
    };
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

#[allow(clippy::too_many_arguments)]
fn plan_check_rr_from_selection(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    target_backend: TargetBackend,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
    selection: ResolvedCheckSelection,
    user_log: &UserLog,
) -> anyhow::Result<(rr_build::BuildMeta, rr_build::BuildInput)> {
    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        Some(target_backend),
        target_dir,
        RunMode::Check,
    );

    let planning_context = rr_build::prepare_resolved_build(
        &preconfig,
        &cli.unstable_feature,
        target_dir,
        user_log,
        &resolve_output,
    )?;
    debug_assert_eq!(planning_context.target_backend(), target_backend);
    rr_build::plan_resolved_build_from_intent(
        preconfig,
        &cli.unstable_feature,
        user_log,
        planning_context,
        selection.into_user_intent()?,
        mooncake_bin_dir,
        resolve_output,
    )
}

pub(crate) fn resolve_check_target_selections(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    user_log: &UserLog,
) -> anyhow::Result<Vec<TargetPackageGroup>> {
    if let Some(target_backend) = selected_target_backend {
        let packages = resolve_selected_packages(
            resolve_output,
            cmd,
            source_dir,
            Some(target_backend),
            user_log,
        )?;
        return Ok(vec![TargetPackageGroup {
            target_backend,
            packages,
        }]);
    }

    let selected = resolve_selected_packages(resolve_output, cmd, source_dir, None, user_log)?;
    let selections = group_packages_by_preferred_backend(resolve_output, selected);

    let mut filtered = Vec::new();
    for selection in selections {
        let packages = filter_packages_for_backend(
            resolve_output,
            selection.packages,
            selection.target_backend,
            user_log,
        )?;
        if !packages.is_empty() {
            filtered.push(TargetPackageGroup {
                target_backend: selection.target_backend,
                packages,
            });
        }
    }

    Ok(filtered)
}

fn resolve_selected_packages(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_backend: Option<TargetBackend>,
    user_log: &UserLog,
) -> anyhow::Result<Vec<PackageId>> {
    if let Some(filter_path) = cmd.package_path.as_deref() {
        let (dir, _) = canonicalize_with_filename(&source_dir.join(filter_path))?;
        let pkg = filter_pkg_by_dir(resolve_output, &dir)?;
        if let Some(target_backend) = target_backend {
            ensure_package_supports_backend(resolve_output, pkg, target_backend)?;
        }
        return Ok(vec![pkg]);
    }

    if !cmd.path.is_empty() {
        if let Some(target_backend) = target_backend {
            return select_supported_packages(resolve_output, &cmd.path, target_backend, user_log);
        }
        return Ok(select_packages(&cmd.path, user_log, |dir| {
            filter_pkg_by_dir(resolve_output, dir)
        })?
        .into_iter()
        .map(|(_, pkg_id)| pkg_id)
        .collect());
    }

    Ok(rr_build::local_packages(resolve_output)
        .filter(|&pkg| {
            target_backend
                .is_none_or(|backend| package_supports_backend(resolve_output, pkg, backend))
        })
        .collect())
}

fn filter_packages_for_backend(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    packages: Vec<PackageId>,
    target_backend: TargetBackend,
    user_log: &UserLog,
) -> anyhow::Result<Vec<PackageId>> {
    let mut supported = Vec::new();
    let mut unsupported = Vec::new();

    for pkg in packages {
        if package_supports_backend(resolve_output, pkg, target_backend) {
            supported.push(pkg);
        } else {
            unsupported.push(pkg);
        }
    }

    if supported.is_empty() && !unsupported.is_empty() {
        if let [pkg] = unsupported.as_slice() {
            ensure_package_supports_backend(resolve_output, *pkg, target_backend)?;
        } else {
            ensure_packages_support_backend(
                resolve_output,
                unsupported.iter().copied(),
                target_backend,
            )?;
        }
    }

    for pkg in unsupported {
        let pkg_id = pkg;
        let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
        user_log.info(format!(
            "skipping package `{}` because it does not support the selected target backend `{}`. Supported backends: {}",
            pkg.fqn,
            target_backend,
            format_supported_backends(resolve_output, pkg_id),
        ));
    }

    Ok(supported)
}

fn calc_user_intent_from_package_path(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    source_dir: &Path,
    filter_path: &Path,
    target_backend: TargetBackend,
    patch_file: Option<&Path>,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let (dir, _) = canonicalize_with_filename(&source_dir.join(filter_path))?;
    let pkg = filter_pkg_by_dir(resolve_output, &dir)?;
    ensure_package_supports_backend(resolve_output, pkg, target_backend)?;
    let directive =
        rr_build::build_patch_directive_for_package(pkg, false, None, patch_file, false)?;
    Ok((vec![UserIntent::Check(pkg)], directive).into())
}

#[instrument(level = Level::DEBUG, skip_all)]
fn calc_user_intent(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    paths: &[PathBuf],
    target_backend: TargetBackend,
    patch_file: Option<&Path>,
    user_log: &UserLog,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    if !paths.is_empty() {
        let selected = select_supported_packages(resolve_output, paths, target_backend, user_log)?;
        let directive = build_directive_for_selected_packages(&selected, patch_file)?;
        Ok((
            selected.into_iter().map(UserIntent::Check).collect(),
            directive,
        )
            .into())
    } else {
        let intents: Vec<_> = rr_build::local_packages(resolve_output)
            .filter(|&pkg| package_supports_backend(resolve_output, pkg, target_backend))
            .map(UserIntent::Check)
            .collect();
        Ok(intents.into())
    }
}

fn build_directive_for_selected_packages(
    selected: &[moonbuild_rupes_recta::model::PackageId],
    patch_file: Option<&Path>,
) -> anyhow::Result<moonbuild_rupes_recta::build_plan::InputDirective> {
    if let [pkg] = selected {
        return rr_build::build_patch_directive_for_package(*pkg, false, None, patch_file, false);
    }

    if patch_file.is_some() {
        anyhow::bail!("`--patch-file` requires the selector to resolve to a single package");
    }
    Ok(Default::default())
}
