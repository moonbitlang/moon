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

use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, bail};
use chrono::Local;
use moonbuild_rupes_recta::model::RunBackend;
use moonutil::{
    common::{RunMode, SurfaceTarget, TargetBackend},
    cond_expr::OptLevel,
};
use serde::Serialize;

use super::{RunSubcommand, UniversalFlags};
use crate::cli::run::{BuildRunExecutableOptions, build_run_executable};

const DEFAULT_TOP: usize = 12;
const MIN_ACTIONABLE_SAMPLES: usize = 100;
pub(crate) const MOON_TEST_PROFILE_COMMAND: &str = "`moon test --profile`";
const PROFILE_TEST_PERFORMANCE_ONLY_MESSAGE: &str = "Profile mode reports performance only; run `moon test` without `--profile` for pass/fail validation.";

#[derive(Debug, Clone, Serialize)]
struct ProfileAgentReport {
    schema_version: u32,
    report_kind: String,
    producer: ProfileProducer,
    target: ProfileTarget,
    artifacts: ProfileArtifacts,
    summary: ProfileSummary,
    rankings: ProfileRankings,
    observations: Vec<ProfileObservation>,
    data_quality: ProfileDataQuality,
    executables: Vec<ProfileExecutableReport>,
    analysis_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileProducer {
    tool: String,
    profiler_backend: String,
    template: String,
    export_format: String,
    parser: String,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileTarget {
    command: String,
    run_mode: String,
    backend: String,
    optimization_profile: String,
    executable: Option<PathBuf>,
    args: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct ProfileArtifacts {
    trace: Option<PathBuf>,
    time_profile_xml: Option<PathBuf>,
    stdout: Option<PathBuf>,
    json_report: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileSummary {
    executables: usize,
    observed_rows: usize,
    running_samples: usize,
    profiled_samples: usize,
    sample_weight_ms: f64,
    profiled_time_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileRankings {
    self_time: Vec<ProfileEntry>,
    inclusive_time: Vec<ProfileEntry>,
    runtime_leaf_attributed_to_user: Vec<ProfileAttributionEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileEntry {
    rank: usize,
    symbol: String,
    mangled_symbol: String,
    samples: usize,
    time_ms: f64,
    percent_of_profiled_samples: f64,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileAttributionEntry {
    rank: usize,
    user_symbol: String,
    user_mangled_symbol: String,
    runtime_symbol: String,
    runtime_mangled_symbol: String,
    samples: usize,
    time_ms: f64,
    percent_of_profiled_samples: f64,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileObservation {
    kind: String,
    rank: usize,
    symbol: String,
    mangled_symbol: String,
    samples: usize,
    time_ms: f64,
    percent_of_profiled_samples: f64,
    note: String,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileDataQuality {
    non_running_samples_ignored: usize,
    missing_stack_samples: usize,
    samples_without_profile_symbols: usize,
    inclusive_symbols_suppressed_from_ranking: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileExecutableReport {
    name: String,
    target: ProfileTarget,
    artifacts: ProfileArtifacts,
    summary: ProfileSummary,
    rankings: ProfileRankings,
    observations: Vec<ProfileObservation>,
    data_quality: ProfileDataQuality,
}

#[derive(Debug, Default, Clone)]
struct ParsedProfile {
    observed_rows: usize,
    running_samples: usize,
    total_samples: usize,
    sample_weight_ms: f64,
    profiled_time_ms: f64,
    non_running_samples_ignored: usize,
    missing_stack_samples: usize,
    samples_without_profile_symbols: usize,
    self_counts: HashMap<String, usize>,
    self_ms: HashMap<String, f64>,
    inclusive_counts: HashMap<String, usize>,
    inclusive_ms: HashMap<String, f64>,
    runtime_attribution_counts: HashMap<RuntimeAttributionKey, usize>,
    runtime_attribution_ms: HashMap<RuntimeAttributionKey, f64>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct RuntimeAttributionKey {
    user_symbol: String,
    runtime_symbol: String,
}

struct CapturedProfile {
    name: String,
    output_dir: PathBuf,
    target: ProfileTarget,
    artifacts: ProfileArtifacts,
    parsed: ParsedProfile,
}

pub(crate) struct ProfileRequest {
    /// Native executable that xctrace should launch.
    pub(crate) executable: PathBuf,
    /// Arguments passed after the executable in the profiled invocation.
    pub(crate) args: Vec<String>,
    /// Directory where the trace, exported XML, stdout, and JSON report are written.
    pub(crate) output_dir: PathBuf,
    /// Moon command mode that requested the profile.
    pub(crate) run_mode: RunMode,
    /// Backend used to build the profiled executable.
    pub(crate) target_backend: RunBackend,
    /// Optimization profile used to build the profiled executable.
    pub(crate) opt_level: OptLevel,
}

pub(crate) fn run_profiled_run(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    ensure_profile_available("`moon run --profile`")?;

    if cmd.command.is_some() {
        bail!("`moon run --profile` does not support inline `-e` source");
    }
    if cmd.package_or_mbt_file.as_deref() == Some("-") {
        bail!("`moon run --profile` does not support stdin source");
    }

    run_profile_materialized(cli, cmd)
}

fn run_profile_materialized(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    let run_cmd = profile_run_subcommand(cmd.clone())?;
    let mut built = build_run_executable(
        cli,
        &run_cmd,
        BuildRunExecutableOptions {
            // Profiling needs a stable executable path for xctrace to launch.
            // The TCC fast path may run directly from generated C instead.
            try_tcc_run: false,
            // The dry-run output should show the profiled invocation, not the
            // plain executable command that `moon run` would normally print.
            print_dry_run_run_command: false,
        },
    )?;
    built.ensure_build_success()?;

    if built.target_backend != RunBackend::Native {
        bail!("`moon run --profile` currently supports only the native backend");
    }

    let output_dir = default_output_dir(
        &built.target_dir,
        built.target_backend,
        built.opt_level,
        RunMode::Run,
        &built.executable,
    );
    if !cli.dry_run {
        built.release_lock();
    }
    let request = ProfileRequest {
        run_mode: RunMode::Run,
        target_backend: built.target_backend,
        opt_level: built.opt_level,
        executable: built.executable,
        args: cmd.args,
        output_dir,
    };

    profile_executable(cli, request)?;
    Ok(0)
}

/// Record, export, parse, and report one already-built executable.
pub(crate) fn profile_executable(
    cli: &UniversalFlags,
    request: ProfileRequest,
) -> anyhow::Result<()> {
    let Some(captured) = capture_profile_executable(cli, request)? else {
        return Ok(());
    };
    let json_path = captured.output_dir.join("profile.json");
    let report = build_report(
        captured.parsed,
        captured.target,
        with_json_report(captured.artifacts, json_path.clone()),
        Vec::new(),
    );
    let terminal_report = render_terminal_report(&report);
    print!("{terminal_report}");
    std::fs::write(&json_path, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write profile json `{}`", json_path.display()))?;

    Ok(())
}

fn capture_profile_executable(
    cli: &UniversalFlags,
    request: ProfileRequest,
) -> anyhow::Result<Option<CapturedProfile>> {
    let ProfileRequest {
        executable,
        args,
        output_dir,
        run_mode,
        target_backend,
        opt_level,
    } = request;
    let trace_path = output_dir.join("profile.trace");
    let xml_path = output_dir.join("time-profile.xml");
    let stdout_path = output_dir.join("stdout.txt");

    if cli.dry_run {
        print_xctrace_record_command(&trace_path, &stdout_path, &executable, &args);
        print_xctrace_export_command(&trace_path, &xml_path);
        return Ok(None);
    }

    std::fs::create_dir_all(&output_dir).with_context(|| {
        format!(
            "failed to create profile output directory `{}`",
            output_dir.display()
        )
    })?;

    if trace_path.exists() {
        bail!(
            "profile trace `{}` already exists; try again or remove the profile output directory",
            trace_path.display()
        );
    }

    run_xctrace_record(&trace_path, &stdout_path, &executable, &args)?;
    run_xctrace_export(&trace_path, &xml_path)?;

    let parsed = parse_xctrace_time_profile(&xml_path)?;
    Ok(Some(CapturedProfile {
        name: executable_profile_name(&executable),
        output_dir,
        target: profile_target(run_mode, target_backend, opt_level, Some(executable), args),
        artifacts: ProfileArtifacts {
            trace: Some(trace_path),
            time_profile_xml: Some(xml_path),
            stdout: Some(stdout_path),
            json_report: None,
        },
        parsed,
    }))
}

fn profile_run_subcommand(cmd: RunSubcommand) -> anyhow::Result<RunSubcommand> {
    let mut build_flags = cmd.build_flags;
    if !build_flags.target.is_empty()
        && build_flags.resolve_single_target_backend()? != Some(TargetBackend::Native)
    {
        bail!("`moon run --profile` currently supports only `--target native`");
    }
    // Time Profiler records a native process. Build release-with-symbols by
    // default so samples are useful without requiring extra flags from users.
    build_flags.target = vec![SurfaceTarget::Native];
    if !build_flags.debug && !build_flags.release {
        build_flags.release = true;
    }
    if !build_flags.strip && !build_flags.no_strip {
        build_flags.no_strip = true;
    }

    Ok(RunSubcommand {
        package_or_mbt_file: cmd.package_or_mbt_file,
        command: cmd.command,
        build_flags,
        args: cmd.args,
        auto_sync_flags: cmd.auto_sync_flags,
        build_only: false,
        profile: false,
    })
}

pub(crate) fn profile_test_invocations(
    cli: &UniversalFlags,
    target_dir: &Path,
    build_meta: &crate::rr_build::BuildMeta,
    filter: &crate::run::TestFilter,
    include_skipped: bool,
) -> Result<i32, anyhow::Error> {
    ensure_profile_available(MOON_TEST_PROFILE_COMMAND)?;

    if build_meta.target_backend != RunBackend::Native {
        bail!("{MOON_TEST_PROFILE_COMMAND} currently supports only the native backend");
    }

    let invocations =
        crate::run::collect_test_invocations(build_meta, filter, include_skipped, false)?;
    if invocations.is_empty() {
        println!("No test executables matched the profile filters.");
        return Ok(0);
    }

    let session_dir = default_test_profile_session_dir(
        target_dir,
        build_meta.target_backend,
        build_meta.opt_level,
    );
    let mut captured_profiles = Vec::new();
    for (index, invocation) in invocations.into_iter().enumerate() {
        // Each selected test executable is a separate process and therefore a
        // separate xctrace recording. Keep raw trace artifacts separated, then
        // merge the parsed profile statistics into one aggregate report.
        let output_dir =
            test_profile_output_dir_for_executable(&session_dir, index, &invocation.executable);
        if let Some(captured) = capture_profile_executable(
            cli,
            ProfileRequest {
                executable: invocation.executable,
                args: vec![invocation.args.to_cli_args_for_native()],
                output_dir,
                run_mode: RunMode::Test,
                target_backend: build_meta.target_backend,
                opt_level: build_meta.opt_level,
            },
        )? {
            captured_profiles.push(captured);
        }
    }
    if !cli.dry_run {
        let json_path = session_dir.join("profile.json");
        let report = build_test_report(
            build_meta.target_backend,
            build_meta.opt_level,
            with_json_report(ProfileArtifacts::default(), json_path.clone()),
            captured_profiles,
        );
        let terminal_report = render_terminal_report(&report);
        print!("{terminal_report}");
        std::fs::write(&json_path, serde_json::to_string_pretty(&report)?)
            .with_context(|| format!("failed to write profile json `{}`", json_path.display()))?;

        // Profile mode intentionally focuses on performance. xctrace owns
        // process execution here, and the profiling report is the primary
        // output; users can inspect the captured stdout files if they need the
        // underlying test output.
        println!("{PROFILE_TEST_PERFORMANCE_ONLY_MESSAGE}");
    }
    Ok(0)
}

fn default_test_profile_session_dir(
    target_dir: &Path,
    target_backend: RunBackend,
    opt_level: OptLevel,
) -> PathBuf {
    output_kind_dir(target_dir, target_backend, opt_level, "profile")
        .join("test")
        .join(profile_timestamp())
}

fn test_profile_output_dir_for_executable(
    session_dir: &Path,
    index: usize,
    executable: &Path,
) -> PathBuf {
    session_dir.join(format!(
        "{:03}-{}",
        index + 1,
        executable_profile_name(executable)
    ))
}

fn sanitize_path_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() || out.chars().all(|c| c == '.') {
        "profile".to_string()
    } else {
        out
    }
}

/// Place profile output beside the canonical artifact layout for the run mode.
pub(crate) fn default_output_dir(
    target_dir: &Path,
    target_backend: RunBackend,
    opt_level: OptLevel,
    artifact_run_mode: RunMode,
    executable: &Path,
) -> PathBuf {
    default_output_parent_dir(
        target_dir,
        target_backend,
        opt_level,
        artifact_run_mode,
        executable,
    )
    .join(profile_timestamp())
}

fn default_output_parent_dir(
    target_dir: &Path,
    target_backend: RunBackend,
    opt_level: OptLevel,
    artifact_run_mode: RunMode,
    executable: &Path,
) -> PathBuf {
    let build_root = output_kind_dir(
        target_dir,
        target_backend,
        opt_level,
        artifact_run_mode.to_dir_name(),
    );
    let mut profile_dir = output_kind_dir(target_dir, target_backend, opt_level, "profile");
    let executable_dir = executable.parent().unwrap_or(executable);
    if let Some(package_dir) = strip_prefix_lenient(executable_dir, &build_root)
        && !package_dir.as_os_str().is_empty()
    {
        profile_dir.push(package_dir);
    } else {
        let stem = executable
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("profile");
        profile_dir.push(sanitize_path_component(stem));
    }
    profile_dir
}

fn profile_timestamp() -> String {
    Local::now().format("%Y%m%d-%H%M%S").to_string()
}

fn output_kind_dir(
    target_dir: &Path,
    target_backend: RunBackend,
    opt_level: OptLevel,
    kind: &str,
) -> PathBuf {
    let profile = match opt_level {
        OptLevel::Debug => "debug",
        OptLevel::Release => "release",
    };
    target_dir
        .join(target_backend.to_target().to_dir_name())
        .join(profile)
        .join(kind)
}

fn strip_prefix_lenient<'a>(path: &'a Path, prefix: &Path) -> Option<&'a Path> {
    path.strip_prefix(prefix).ok().or_else(|| {
        strip_current_dir(path)
            .strip_prefix(strip_current_dir(prefix))
            .ok()
    })
}

fn strip_current_dir(path: &Path) -> &Path {
    path.strip_prefix(".").unwrap_or(path)
}

pub(crate) fn ensure_profile_available(command_name: &str) -> anyhow::Result<()> {
    if !cfg!(target_os = "macos") {
        bail!("{command_name} currently supports macOS only");
    }

    ensure_xctrace_available(command_name)
}

fn ensure_xctrace_available(command_name: &str) -> anyhow::Result<()> {
    let output = Command::new("xcrun")
        .args(["xctrace", "version"])
        .output()
        .context("failed to execute `xcrun xctrace version`")?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!(
        "{command_name} on macOS requires `xcrun xctrace`.\n\n\
         xctrace failed with:\n{}\n\
         Try:\n  xcode-select --install\n\n\
         If xctrace reports an Xcode license error:\n  sudo xcodebuild -license accept",
        stderr.trim()
    )
}

fn run_xctrace_record(
    trace_path: &Path,
    stdout_path: &Path,
    executable: &Path,
    args: &[String],
) -> anyhow::Result<()> {
    let mut cmd = xctrace_record_command(trace_path, stdout_path, executable, args);
    let output = cmd
        .output()
        .context("failed to execute `xcrun xctrace record`")?;
    if !output.status.success() {
        bail!(
            "`xcrun xctrace record` failed with {}\n{}{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn run_xctrace_export(trace_path: &Path, xml_path: &Path) -> anyhow::Result<()> {
    let mut cmd = xctrace_export_command(trace_path, xml_path);
    let output = cmd
        .output()
        .context("failed to execute `xcrun xctrace export`")?;
    if !output.status.success() {
        bail!(
            "`xcrun xctrace export` failed with {}\n{}{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn xctrace_record_command(
    trace_path: &Path,
    stdout_path: &Path,
    executable: &Path,
    args: &[String],
) -> Command {
    let mut cmd = Command::new("xcrun");
    cmd.args([
        "xctrace",
        "record",
        "--quiet",
        "--template",
        "Time Profiler",
        "--no-prompt",
        "--output",
    ]);
    cmd.arg(trace_path);
    cmd.arg("--target-stdout");
    cmd.arg(stdout_path);
    cmd.arg("--launch");
    cmd.arg("--");
    cmd.arg(executable);
    cmd.args(args);
    cmd
}

fn xctrace_export_command(trace_path: &Path, xml_path: &Path) -> Command {
    let mut cmd = Command::new("xcrun");
    cmd.args(["xctrace", "export", "--quiet", "--input"]);
    cmd.arg(trace_path);
    cmd.args([
        "--xpath",
        // Export only Time Profiler samples; the full trace export is much
        // larger and contains many unrelated tables.
        "/trace-toc/run[@number=\"1\"]/data/table[@schema=\"time-profile\"]",
        "--output",
    ]);
    cmd.arg(xml_path);
    cmd
}

fn print_xctrace_record_command(
    trace_path: &Path,
    stdout_path: &Path,
    executable: &Path,
    args: &[String],
) {
    let cmd = xctrace_record_command(trace_path, stdout_path, executable, args);
    print_command(cmd);
}

fn print_xctrace_export_command(trace_path: &Path, xml_path: &Path) {
    let cmd = xctrace_export_command(trace_path, xml_path);
    print_command(cmd);
}

fn print_command(cmd: Command) {
    let parts = std::iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    println!(
        "{}",
        moonutil::shlex::join_unix(parts.iter().map(String::as_str))
    );
}

fn parse_xctrace_time_profile(path: &Path) -> anyhow::Result<ParsedProfile> {
    let xml = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read xctrace export `{}`", path.display()))?;
    Ok(parse_xctrace_time_profile_xml(&xml))
}

fn parse_xctrace_time_profile_xml(xml: &str) -> ParsedProfile {
    // xctrace repeats common values by reference, for example
    // `<thread-state ref="..."/>` and `<stack ref="..."/>`. Keep a small
    // cache of previously seen values while walking rows in export order.
    let mut states: HashMap<String, String> = HashMap::new();
    let mut weights: HashMap<String, f64> = HashMap::new();
    let mut frames: HashMap<String, String> = HashMap::new();
    let mut stacks: HashMap<String, Vec<String>> = HashMap::new();
    let mut parsed = ParsedProfile {
        sample_weight_ms: 1.0,
        ..Default::default()
    };

    for row in rows(xml) {
        parsed.observed_rows += 1;

        for tag in tags(row, "thread-state") {
            if let (Some(id), Some(fmt)) = (attr(tag, "id"), attr(tag, "fmt")) {
                states.insert(id, unescape_xml_attr(&fmt));
            }
        }
        for tag in tags(row, "weight") {
            if let Some(id) = attr(tag, "id") {
                let value = tag_text_after(row, tag).unwrap_or_default();
                if let Ok(ns) = value.trim().parse::<u64>() {
                    let ms = ns as f64 / 1_000_000.0;
                    weights.insert(id, ms);
                    parsed.sample_weight_ms = ms;
                }
            }
        }

        let stack = parse_or_resolve_stack(row, &mut frames, &mut stacks);

        let state = first_tag(row, "thread-state")
            .and_then(|tag| resolve_value(tag, &states, "fmt"))
            .unwrap_or_default();
        if state != "Running" {
            parsed.non_running_samples_ignored += 1;
            continue;
        }
        parsed.running_samples += 1;

        let weight = first_tag(row, "weight")
            .and_then(|tag| resolve_value(tag, &weights, ""))
            .unwrap_or(parsed.sample_weight_ms);

        let Some(stack) = stack else {
            parsed.missing_stack_samples += 1;
            continue;
        };
        let stack = stack
            .into_iter()
            .filter(|name| is_profile_symbol(name))
            .collect::<Vec<_>>();
        if stack.is_empty() {
            parsed.samples_without_profile_symbols += 1;
            continue;
        }

        parsed.total_samples += 1;
        parsed.profiled_time_ms += weight;
        record_runtime_leaf_attribution(&mut parsed, &stack, weight);

        let top = stack[0].clone();
        *parsed.self_counts.entry(top.clone()).or_default() += 1;
        *parsed.self_ms.entry(top).or_default() += weight;

        let mut seen = std::collections::HashSet::new();
        for symbol in stack {
            if seen.insert(symbol.clone()) {
                *parsed.inclusive_counts.entry(symbol.clone()).or_default() += 1;
                *parsed.inclusive_ms.entry(symbol).or_default() += weight;
            }
        }
    }

    parsed
}

fn record_runtime_leaf_attribution(parsed: &mut ParsedProfile, stack: &[String], weight: f64) {
    let Some((runtime_symbol, user_symbol)) = runtime_leaf_attribution(stack) else {
        return;
    };
    let key = RuntimeAttributionKey {
        user_symbol,
        runtime_symbol,
    };
    *parsed
        .runtime_attribution_counts
        .entry(key.clone())
        .or_default() += 1;
    *parsed.runtime_attribution_ms.entry(key).or_default() += weight;
}

fn runtime_leaf_attribution(stack: &[String]) -> Option<(String, String)> {
    let runtime_symbol = stack.first()?;
    if !is_runtime_leaf_symbol(runtime_symbol) {
        return None;
    }
    let user_symbol = stack.iter().skip(1).find(|name| is_user_frame(name))?;
    Some((runtime_symbol.clone(), user_symbol.clone()))
}

fn rows(xml: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<row") {
        rest = &rest[start..];
        let Some(end) = rest.find("</row>") else {
            break;
        };
        let row = &rest[..end + "</row>".len()];
        out.push(row);
        rest = &rest[end + "</row>".len()..];
    }
    out
}

fn tags<'a>(row: &'a str, tag: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let needle = format!("<{tag}");
    let mut rest = row;
    while let Some(start) = rest.find(&needle) {
        rest = &rest[start..];
        let Some(end) = rest.find('>') else { break };
        out.push(&rest[..=end]);
        rest = &rest[end + 1..];
    }
    out
}

fn first_tag<'a>(row: &'a str, tag: &str) -> Option<&'a str> {
    tags(row, tag).into_iter().next()
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn tag_text_after(row: &str, tag: &str) -> Option<String> {
    let start = row.find(tag)? + tag.len();
    let end = row[start..].find('<')?;
    Some(row[start..start + end].to_string())
}

fn resolve_value<T: Clone + std::str::FromStr>(
    tag: &str,
    refs: &HashMap<String, T>,
    attr_name: &str,
) -> Option<T> {
    if let Some(reference) = attr(tag, "ref") {
        return refs.get(&reference).cloned();
    }
    if attr_name.is_empty() {
        return None;
    }
    attr(tag, attr_name)?.parse().ok()
}

fn parse_or_resolve_stack(
    row: &str,
    frames: &mut HashMap<String, String>,
    stacks: &mut HashMap<String, Vec<String>>,
) -> Option<Vec<String>> {
    // Different Xcode versions/export shapes expose the backtrace column under
    // different tags, even though the schema mnemonic is usually `stack`.
    let stack_tag = first_tag(row, "stack")
        .or_else(|| first_tag(row, "tagged-backtrace"))
        .or_else(|| first_tag(row, "backtrace"))?;
    if let Some(reference) = attr(stack_tag, "ref") {
        return stacks.get(&reference).cloned();
    }

    for frame_tag in tags(row, "frame") {
        if let (Some(id), Some(name)) = (attr(frame_tag, "id"), attr(frame_tag, "name")) {
            frames.insert(id, unescape_xml_attr(&name));
        }
    }

    let mut stack = Vec::new();
    for frame_tag in tags(row, "frame") {
        if let Some(reference) = attr(frame_tag, "ref") {
            if let Some(name) = frames.get(&reference) {
                stack.push(name.clone());
            }
        } else if let Some(name) = attr(frame_tag, "name") {
            stack.push(unescape_xml_attr(&name));
        }
    }

    if let Some(id) = attr(stack_tag, "id") {
        stacks.insert(id, stack.clone());
    }
    Some(stack)
}

fn unescape_xml_attr(input: &str) -> String {
    input
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn is_profile_symbol(name: &str) -> bool {
    name.starts_with("_M0")
        || name.starts_with("moonbit_")
        || name.starts_with("_mi_")
        || name.starts_with("mi_")
        || name.starts_with("OUTLINED_FUNCTION_")
        || name == "<deduplicated_symbol>"
        || name == "main"
}

fn build_report(
    parsed: ParsedProfile,
    target: ProfileTarget,
    artifacts: ProfileArtifacts,
    executables: Vec<ProfileExecutableReport>,
) -> ProfileAgentReport {
    let executable_count = if executables.is_empty() {
        usize::from(target.executable.is_some())
    } else {
        executables.len()
    };
    let (summary, rankings, observations, data_quality) =
        profile_sections(parsed, executable_count);
    ProfileAgentReport {
        schema_version: 2,
        report_kind: "moon.profile.agent".to_string(),
        producer: ProfileProducer {
            tool: "moon".to_string(),
            profiler_backend: "xctrace".to_string(),
            template: "Time Profiler".to_string(),
            export_format: "xctrace time-profile XML".to_string(),
            parser: "moon-xctrace-time-profile-v1".to_string(),
        },
        target,
        artifacts,
        summary,
        rankings,
        observations,
        data_quality,
        executables,
        analysis_notes: vec![
            "self_time ranks leaf samples by the top frame in each sampled stack.".to_string(),
            "inclusive_time counts a symbol at most once per sampled stack and suppresses generic entrypoint/runtime dispatcher symbols.".to_string(),
            "runtime_leaf_attributed_to_user attributes selected runtime/library leaf samples to the nearest non-runtime MoonBit frame higher in the sampled stack.".to_string(),
            "percent_of_profiled_samples uses profiled_samples after ignoring non-running samples and running stacks without MoonBit/runtime symbols.".to_string(),
            "For moon test --profile, top-level rankings merge all profiled test executables; per-executable rankings remain under executables[].".to_string(),
            "Open artifacts.trace for a single-executable report, or executables[].artifacts.trace for a merged test report, in Instruments when call-tree context or timeline ordering is needed.".to_string(),
        ],
    }
}

fn build_test_report(
    target_backend: RunBackend,
    opt_level: OptLevel,
    artifacts: ProfileArtifacts,
    captured_profiles: Vec<CapturedProfile>,
) -> ProfileAgentReport {
    let mut merged = ParsedProfile::default();
    let mut executable_reports = Vec::with_capacity(captured_profiles.len());
    for captured in captured_profiles {
        merge_profile(&mut merged, &captured.parsed);
        executable_reports.push(build_executable_report(captured));
    }
    build_report(
        merged,
        profile_target(RunMode::Test, target_backend, opt_level, None, Vec::new()),
        artifacts,
        executable_reports,
    )
}

fn build_executable_report(captured: CapturedProfile) -> ProfileExecutableReport {
    let (summary, rankings, observations, data_quality) = profile_sections(captured.parsed, 1);
    ProfileExecutableReport {
        name: captured.name,
        target: captured.target,
        artifacts: captured.artifacts,
        summary,
        rankings,
        observations,
        data_quality,
    }
}

fn profile_sections(
    parsed: ParsedProfile,
    executable_count: usize,
) -> (
    ProfileSummary,
    ProfileRankings,
    Vec<ProfileObservation>,
    ProfileDataQuality,
) {
    let total = parsed.total_samples.max(1) as f64;
    let summary = ProfileSummary {
        executables: executable_count,
        observed_rows: parsed.observed_rows,
        running_samples: parsed.running_samples,
        profiled_samples: parsed.total_samples,
        sample_weight_ms: parsed.sample_weight_ms,
        profiled_time_ms: parsed.profiled_time_ms,
    };
    let data_quality = ProfileDataQuality {
        non_running_samples_ignored: parsed.non_running_samples_ignored,
        missing_stack_samples: parsed.missing_stack_samples,
        samples_without_profile_symbols: parsed.samples_without_profile_symbols,
        inclusive_symbols_suppressed_from_ranking: vec![
            "main".to_string(),
            "____moonbit__main".to_string(),
            "@moonbitlang/async/".to_string(),
        ],
        warnings: profile_warnings(&parsed),
    };
    let self_time = ranked_entries(parsed.self_counts, parsed.self_ms, total, |_| true);
    let inclusive_time = ranked_entries(
        parsed.inclusive_counts,
        parsed.inclusive_ms,
        total,
        |symbol| !skip_inclusive_symbol(symbol),
    );
    let runtime_leaf_attributed_to_user = ranked_attribution_entries(
        parsed.runtime_attribution_counts,
        parsed.runtime_attribution_ms,
        total,
    );
    let rankings = ProfileRankings {
        self_time,
        inclusive_time,
        runtime_leaf_attributed_to_user,
    };
    let observations = profile_observations(&rankings);
    (summary, rankings, observations, data_quality)
}

fn merge_profile(merged: &mut ParsedProfile, parsed: &ParsedProfile) {
    merged.observed_rows += parsed.observed_rows;
    merged.running_samples += parsed.running_samples;
    merged.total_samples += parsed.total_samples;
    merged.profiled_time_ms += parsed.profiled_time_ms;
    merged.non_running_samples_ignored += parsed.non_running_samples_ignored;
    merged.missing_stack_samples += parsed.missing_stack_samples;
    merged.samples_without_profile_symbols += parsed.samples_without_profile_symbols;

    for (symbol, samples) in &parsed.self_counts {
        *merged.self_counts.entry(symbol.clone()).or_default() += *samples;
    }
    for (symbol, time_ms) in &parsed.self_ms {
        *merged.self_ms.entry(symbol.clone()).or_default() += *time_ms;
    }
    for (symbol, samples) in &parsed.inclusive_counts {
        *merged.inclusive_counts.entry(symbol.clone()).or_default() += *samples;
    }
    for (symbol, time_ms) in &parsed.inclusive_ms {
        *merged.inclusive_ms.entry(symbol.clone()).or_default() += *time_ms;
    }
    for (key, samples) in &parsed.runtime_attribution_counts {
        *merged
            .runtime_attribution_counts
            .entry(key.clone())
            .or_default() += *samples;
    }
    for (key, time_ms) in &parsed.runtime_attribution_ms {
        *merged
            .runtime_attribution_ms
            .entry(key.clone())
            .or_default() += *time_ms;
    }

    if merged.total_samples > 0 {
        merged.sample_weight_ms = merged.profiled_time_ms / merged.total_samples as f64;
    } else if merged.sample_weight_ms == 0.0 {
        merged.sample_weight_ms = parsed.sample_weight_ms;
    }
}

fn profile_target(
    run_mode: RunMode,
    target_backend: RunBackend,
    opt_level: OptLevel,
    executable: Option<PathBuf>,
    args: Vec<String>,
) -> ProfileTarget {
    ProfileTarget {
        command: format!("moon {} --profile", run_mode_label(run_mode)),
        run_mode: run_mode_label(run_mode).to_string(),
        backend: target_backend.to_target().to_dir_name().to_string(),
        optimization_profile: opt_level_label(opt_level).to_string(),
        executable,
        args,
    }
}

fn with_json_report(mut artifacts: ProfileArtifacts, json_path: PathBuf) -> ProfileArtifacts {
    artifacts.json_report = Some(json_path);
    artifacts
}

fn executable_profile_name(executable: &Path) -> String {
    let stem = executable
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("profile");
    sanitize_path_component(stem)
}

fn ranked_entries(
    counts: HashMap<String, usize>,
    times: HashMap<String, f64>,
    total_samples: f64,
    keep: impl Fn(&str) -> bool,
) -> Vec<ProfileEntry> {
    let mut entries = counts
        .into_iter()
        .filter(|(symbol, _)| keep(symbol))
        .map(|(mangled_symbol, samples)| {
            let time_ms = times
                .get(&mangled_symbol)
                .copied()
                .unwrap_or(samples as f64);
            let symbol = demangle_symbol(&mangled_symbol);
            ProfileEntry {
                rank: 0,
                symbol,
                mangled_symbol,
                samples,
                time_ms,
                percent_of_profiled_samples: samples as f64 * 100.0 / total_samples,
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        b.samples
            .cmp(&a.samples)
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    for (index, entry) in entries.iter_mut().enumerate() {
        entry.rank = index + 1;
    }
    entries
}

fn ranked_attribution_entries(
    counts: HashMap<RuntimeAttributionKey, usize>,
    times: HashMap<RuntimeAttributionKey, f64>,
    total_samples: f64,
) -> Vec<ProfileAttributionEntry> {
    let mut entries = counts
        .into_iter()
        .map(|(key, samples)| {
            let time_ms = times.get(&key).copied().unwrap_or(samples as f64);
            ProfileAttributionEntry {
                rank: 0,
                user_symbol: demangle_symbol(&key.user_symbol),
                user_mangled_symbol: key.user_symbol,
                runtime_symbol: demangle_symbol(&key.runtime_symbol),
                runtime_mangled_symbol: key.runtime_symbol,
                samples,
                time_ms,
                percent_of_profiled_samples: samples as f64 * 100.0 / total_samples,
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        b.samples
            .cmp(&a.samples)
            .then_with(|| a.user_symbol.cmp(&b.user_symbol))
            .then_with(|| a.runtime_symbol.cmp(&b.runtime_symbol))
    });
    for (index, entry) in entries.iter_mut().enumerate() {
        entry.rank = index + 1;
    }
    entries
}

fn profile_observations(rankings: &ProfileRankings) -> Vec<ProfileObservation> {
    let mut observations = Vec::new();
    if let Some(entry) = rankings.self_time.first() {
        observations.push(ProfileObservation {
            kind: "highest_self_time".to_string(),
            rank: entry.rank,
            symbol: entry.symbol.clone(),
            mangled_symbol: entry.mangled_symbol.clone(),
            samples: entry.samples,
            time_ms: entry.time_ms,
            percent_of_profiled_samples: entry.percent_of_profiled_samples,
            note: "Largest leaf-frame hotspot; inspect this function for local work.".to_string(),
        });
    }
    if let Some(entry) = rankings.inclusive_time.first() {
        observations.push(ProfileObservation {
            kind: "highest_inclusive_time".to_string(),
            rank: entry.rank,
            symbol: entry.symbol.clone(),
            mangled_symbol: entry.mangled_symbol.clone(),
            samples: entry.samples,
            time_ms: entry.time_ms,
            percent_of_profiled_samples: entry.percent_of_profiled_samples,
            note: "Largest owner/caller hotspot; inspect this function and its callees."
                .to_string(),
        });
    }
    observations
}

fn profile_warnings(parsed: &ParsedProfile) -> Vec<String> {
    let mut warnings = Vec::new();
    if parsed.total_samples == 0 {
        warnings.push(
            "No profiled running samples with MoonBit/runtime symbols were found.".to_string(),
        );
    } else if parsed.total_samples < MIN_ACTIONABLE_SAMPLES {
        let (sample_word, verb) = if parsed.total_samples == 1 {
            ("sample", "was")
        } else {
            ("samples", "were")
        };
        warnings.push(format!(
            "Only {} profiled {sample_word} {verb} collected; results may be noisy. Consider profiling a larger workload.",
            parsed.total_samples,
        ));
    }
    if parsed.samples_without_profile_symbols > 0 {
        warnings.push(format!(
            "{} running samples had stacks, but no MoonBit/runtime symbols after filtering.",
            parsed.samples_without_profile_symbols
        ));
    }
    if parsed.missing_stack_samples > 0 {
        warnings.push(format!(
            "{} running samples did not contain a resolved stack.",
            parsed.missing_stack_samples
        ));
    }
    warnings
}

fn run_mode_label(run_mode: RunMode) -> &'static str {
    match run_mode {
        RunMode::Bench => "bench",
        RunMode::Build => "build",
        RunMode::Check => "check",
        RunMode::Prove => "prove",
        RunMode::Run => "run",
        RunMode::Test => "test",
        RunMode::Bundle => "bundle",
        RunMode::Format => "format",
    }
}

fn opt_level_label(opt_level: OptLevel) -> &'static str {
    match opt_level {
        OptLevel::Debug => "debug",
        OptLevel::Release => "release",
    }
}

fn demangle_symbol(symbol: &str) -> String {
    if symbol.starts_with("_M0") {
        moonutil::demangle::demangle_mangled_function_name(symbol)
    } else {
        symbol.to_string()
    }
}

fn is_runtime_leaf_symbol(symbol: &str) -> bool {
    let demangled = demangle_symbol(symbol);
    let lower = demangled.to_ascii_lowercase();
    symbol.starts_with("moonbit_")
        || symbol.starts_with("_mi_")
        || symbol.starts_with("mi_")
        || lower.contains("stringview::")
        || lower.contains("moonbit_make_string")
        || lower.contains("make_string")
        || lower.contains("incref")
        || lower.contains("decref")
}

fn is_user_frame(symbol: &str) -> bool {
    symbol.starts_with("_M0") && !is_runtime_leaf_symbol(symbol) && !skip_inclusive_symbol(symbol)
}

fn skip_inclusive_symbol(symbol: &str) -> bool {
    let demangled = demangle_symbol(symbol);
    demangled == "main"
        || demangled.contains("____moonbit__main")
        || demangled.starts_with("@moonbitlang/async/")
}

fn render_terminal_report(report: &ProfileAgentReport) -> String {
    let mut out = String::new();
    out.push_str("Profile: native macOS Time Profiler\n");
    out.push_str(&format!("Executables: {}\n", report.summary.executables));
    out.push_str(&format!(
        "Samples: {}, sample weight: {:.2}ms\n",
        report.summary.profiled_samples, report.summary.sample_weight_ms
    ));
    if let Some(trace) = &report.artifacts.trace {
        out.push_str(&format!("Trace: {}\n", trace.display()));
    }
    if let Some(stdout) = &report.artifacts.stdout {
        out.push_str(&format!("Program stdout: {}\n", stdout.display()));
    }
    if !report.data_quality.warnings.is_empty() {
        out.push('\n');
        out.push_str("Warnings:\n");
        for warning in &report.data_quality.warnings {
            out.push_str(&format!("  {warning}\n"));
        }
    }
    out.push('\n');
    out.push_str("Top self time:\n");
    render_entries(&mut out, &report.rankings.self_time, DEFAULT_TOP);
    out.push('\n');
    out.push_str("Top inclusive time:\n");
    render_entries(&mut out, &report.rankings.inclusive_time, DEFAULT_TOP);
    out.push('\n');
    out.push_str("Runtime leaf costs attributed to MoonBit callers:\n");
    render_attribution_entries(
        &mut out,
        &report.rankings.runtime_leaf_attributed_to_user,
        DEFAULT_TOP,
    );
    out.push('\n');
    if let Some(json_report) = &report.artifacts.json_report {
        out.push_str(&format!(
            "Detailed JSON report: {}\n",
            json_report.display()
        ));
    }
    if let Some(trace) = &report.artifacts.trace {
        out.push_str("Open the full trace in Instruments:\n");
        out.push_str(&format!("  open {}\n", trace.display()));
    } else if !report.executables.is_empty() {
        out.push_str("Open per-executable traces listed in the JSON report.\n");
    }
    out
}

fn render_entries(out: &mut String, entries: &[ProfileEntry], limit: usize) {
    if entries.is_empty() {
        out.push_str("  no samples\n");
        return;
    }
    for entry in entries.iter().take(limit) {
        out.push_str(&format!(
            "  {:>5.1}% {:>8.2}ms {:>6}  {}\n",
            entry.percent_of_profiled_samples, entry.time_ms, entry.samples, entry.symbol
        ));
    }
}

fn render_attribution_entries(out: &mut String, entries: &[ProfileAttributionEntry], limit: usize) {
    if entries.is_empty() {
        out.push_str("  no attributed runtime leaf samples\n");
        return;
    }
    for entry in entries.iter().take(limit) {
        out.push_str(&format!(
            "  {:>5.1}% {:>8.2}ms {:>6}  {} <- {}\n",
            entry.percent_of_profiled_samples,
            entry.time_ms,
            entry.samples,
            entry.user_symbol,
            entry.runtime_symbol
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use moonbuild_rupes_recta::model::RunBackend;
    use moonutil::{common::RunMode, cond_expr::OptLevel};

    use super::{
        CapturedProfile, PROFILE_TEST_PERFORMANCE_ONLY_MESSAGE, ProfileArtifacts, build_report,
        build_test_report, default_output_dir, parse_xctrace_time_profile_xml, profile_target,
        render_terminal_report, sanitize_path_component, test_profile_output_dir_for_executable,
        with_json_report,
    };

    #[test]
    fn parses_referenced_xctrace_stack_rows() {
        let xml = r#"
<trace-query-result><node>
<row><thread-state id="1" fmt="Running">Running</thread-state><weight id="2" fmt="1.00 ms">1000000</weight><stack id="3" fmt="_M0foo"><frame id="5" name="_M0foo" addr="0x1"/><frame id="6" name="_M0bar" addr="0x2"/></stack></row>
<row><thread-state ref="1"/><weight ref="2"/><stack ref="3"/></row>
<row><thread-state id="7" fmt="Blocked">Blocked</thread-state><weight ref="2"/><stack ref="3"/></row>
</node></trace-query-result>
"#;
        let parsed = parse_xctrace_time_profile_xml(xml);
        assert_eq!(parsed.total_samples, 2);
        assert_eq!(parsed.self_counts["_M0foo"], 2);
        assert_eq!(parsed.inclusive_counts["_M0bar"], 2);
    }

    #[test]
    fn performance_only_message_points_to_plain_test_validation() {
        assert!(PROFILE_TEST_PERFORMANCE_ONLY_MESSAGE.contains("performance only"));
        assert!(PROFILE_TEST_PERFORMANCE_ONLY_MESSAGE.contains("without `--profile`"));
    }

    #[test]
    fn still_accepts_tagged_backtrace_rows() {
        let xml = r#"
<trace-query-result><node>
<row><thread-state id="1" fmt="Running">Running</thread-state><weight id="2" fmt="1.00 ms">1000000</weight><tagged-backtrace id="3" fmt="_M0foo"><backtrace id="4"><frame id="5" name="_M0foo" addr="0x1"/></backtrace></tagged-backtrace></row>
</node></trace-query-result>
"#;
        let parsed = parse_xctrace_time_profile_xml(xml);
        assert_eq!(parsed.total_samples, 1);
        assert_eq!(parsed.self_counts["_M0foo"], 1);
    }

    #[test]
    fn parses_bare_backtrace_rows() {
        let xml = r#"
<trace-query-result><node>
<row><thread-state id="1" fmt="Running">Running</thread-state><weight id="2" fmt="1.00 ms">1000000</weight><backtrace id="3"><frame id="5" name="_M0foo" addr="0x1"/><frame id="6" name="_M0bar" addr="0x2"/></backtrace></row>
<row><thread-state ref="1"/><weight ref="2"/><backtrace ref="3"/></row>
</node></trace-query-result>
"#;
        let parsed = parse_xctrace_time_profile_xml(xml);
        assert_eq!(parsed.total_samples, 2);
        assert_eq!(parsed.self_counts["_M0foo"], 2);
        assert_eq!(parsed.inclusive_counts["_M0bar"], 2);
    }

    #[test]
    fn agent_report_keeps_detailed_json_shape_and_human_stdout() {
        let xml = r#"
<trace-query-result><node>
<row><thread-state id="1" fmt="Running">Running</thread-state><weight id="2" fmt="1.00 ms">1000000</weight><stack id="3"><frame id="5" name="moonbit_hot" addr="0x1"/><frame id="6" name="moonbit_owner" addr="0x2"/></stack></row>
<row><thread-state id="7" fmt="Blocked">Blocked</thread-state><weight ref="2"/><stack ref="3"/></row>
<row><thread-state ref="1"/><weight ref="2"/><stack id="8"><frame id="9" name="libsystem_kernel.dylib" addr="0x3"/></stack></row>
</node></trace-query-result>
"#;
        let parsed = parse_xctrace_time_profile_xml(xml);
        let report = build_report(
            parsed,
            profile_target(
                RunMode::Run,
                RunBackend::Native,
                OptLevel::Release,
                Some(PathBuf::from("./_build/native/release/build/main/main.exe")),
                vec!["--flag".to_string()],
            ),
            with_json_report(
                ProfileArtifacts {
                    trace: Some(PathBuf::from(
                        "./_build/native/release/profile/main/profile.trace",
                    )),
                    time_profile_xml: Some(PathBuf::from(
                        "./_build/native/release/profile/main/time-profile.xml",
                    )),
                    stdout: Some(PathBuf::from(
                        "./_build/native/release/profile/main/stdout.txt",
                    )),
                    json_report: None,
                },
                PathBuf::from("./_build/native/release/profile/main/profile.json"),
            ),
            Vec::new(),
        );

        let terminal_report = render_terminal_report(&report);
        assert!(terminal_report.contains("Top self time:"));
        assert!(terminal_report.contains("Detailed JSON report:"));
        assert!(!terminal_report.contains("Full text report:"));

        let json = serde_json::to_value(&report).expect("serialize profile report");
        assert_eq!(json["schema_version"], 2);
        assert_eq!(json["report_kind"], "moon.profile.agent");
        assert_eq!(json["target"]["command"], "moon run --profile");
        assert_eq!(json["target"]["args"], serde_json::json!(["--flag"]));
        assert_eq!(json["summary"]["executables"], 1);
        assert_eq!(json["summary"]["observed_rows"], 3);
        assert_eq!(json["summary"]["running_samples"], 2);
        assert_eq!(json["summary"]["profiled_samples"], 1);
        assert_eq!(
            json["data_quality"]["non_running_samples_ignored"],
            serde_json::json!(1)
        );
        assert_eq!(
            json["data_quality"]["samples_without_profile_symbols"],
            serde_json::json!(1)
        );
        assert_eq!(json["rankings"]["self_time"][0]["rank"], 1);
        assert_eq!(
            json["rankings"]["self_time"][0]["mangled_symbol"],
            "moonbit_hot"
        );
        assert_eq!(json["observations"][0]["kind"], "highest_self_time");
        assert!(
            json["analysis_notes"]
                .as_array()
                .is_some_and(|notes| !notes.is_empty())
        );
    }

    #[test]
    fn runtime_leaf_attribution_points_to_nearest_moonbit_frame() {
        let xml = r#"
<trace-query-result><node>
<row><thread-state id="1" fmt="Running">Running</thread-state><weight id="2" fmt="1.00 ms">1000000</weight><stack id="3"><frame id="5" name="moonbit_make_string" addr="0x1"/><frame id="6" name="_M0Parser_error_at" addr="0x2"/><frame id="7" name="_M0Parser_outer" addr="0x3"/></stack></row>
</node></trace-query-result>
"#;
        let parsed = parse_xctrace_time_profile_xml(xml);
        let report = build_report(
            parsed,
            profile_target(
                RunMode::Run,
                RunBackend::Native,
                OptLevel::Release,
                Some(PathBuf::from("./_build/native/release/build/main/main.exe")),
                Vec::new(),
            ),
            with_json_report(
                ProfileArtifacts {
                    trace: Some(PathBuf::from(
                        "./_build/native/release/profile/main/profile.trace",
                    )),
                    time_profile_xml: Some(PathBuf::from(
                        "./_build/native/release/profile/main/time-profile.xml",
                    )),
                    stdout: Some(PathBuf::from(
                        "./_build/native/release/profile/main/stdout.txt",
                    )),
                    json_report: None,
                },
                PathBuf::from("./_build/native/release/profile/main/profile.json"),
            ),
            Vec::new(),
        );

        let terminal_report = render_terminal_report(&report);
        assert!(terminal_report.contains("Only 1 profiled sample was collected"));
        assert!(terminal_report.contains("Runtime leaf costs attributed to MoonBit callers:"));
        assert!(terminal_report.contains("<- moonbit_make_string"));

        let json = serde_json::to_value(&report).expect("serialize profile report");
        assert_eq!(
            json["rankings"]["runtime_leaf_attributed_to_user"][0]["runtime_mangled_symbol"],
            "moonbit_make_string"
        );
        assert_eq!(
            json["rankings"]["runtime_leaf_attributed_to_user"][0]["user_mangled_symbol"],
            "_M0Parser_error_at"
        );
        assert_eq!(
            json["data_quality"]["warnings"][0],
            "Only 1 profiled sample was collected; results may be noisy. Consider profiling a larger workload."
        );
    }

    #[test]
    fn test_profile_report_merges_multiple_executables() {
        let first = parse_xctrace_time_profile_xml(
            r#"
<trace-query-result><node>
<row><thread-state id="1" fmt="Running">Running</thread-state><weight id="2" fmt="1.00 ms">1000000</weight><stack id="3"><frame id="5" name="moonbit_shared" addr="0x1"/></stack></row>
</node></trace-query-result>
"#,
        );
        let second = parse_xctrace_time_profile_xml(
            r#"
<trace-query-result><node>
<row><thread-state id="1" fmt="Running">Running</thread-state><weight id="2" fmt="1.00 ms">1000000</weight><stack id="3"><frame id="5" name="moonbit_shared" addr="0x1"/></stack></row>
<row><thread-state ref="1"/><weight ref="2"/><stack id="4"><frame id="6" name="moonbit_other" addr="0x2"/></stack></row>
</node></trace-query-result>
"#,
        );
        let report = build_test_report(
            RunBackend::Native,
            OptLevel::Release,
            with_json_report(
                ProfileArtifacts::default(),
                PathBuf::from("./_build/native/release/profile/test/ts/profile.json"),
            ),
            vec![
                captured_test_profile("main_internal_test", first),
                captured_test_profile("main_blackbox_test", second),
            ],
        );

        let terminal_report = render_terminal_report(&report);
        assert!(terminal_report.contains("Executables: 2"));
        assert!(terminal_report.contains("Open per-executable traces listed in the JSON report."));

        let json = serde_json::to_value(&report).expect("serialize profile report");
        assert_eq!(json["target"]["command"], "moon test --profile");
        assert_eq!(json["target"]["executable"], serde_json::Value::Null);
        assert_eq!(json["summary"]["executables"], 2);
        assert_eq!(json["summary"]["profiled_samples"], 3);
        assert_eq!(
            json["rankings"]["self_time"][0]["mangled_symbol"],
            "moonbit_shared"
        );
        assert_eq!(json["rankings"]["self_time"][0]["samples"], 2);
        assert_eq!(
            json["executables"].as_array().expect("executables").len(),
            2
        );
        assert_eq!(json["executables"][0]["summary"]["profiled_samples"], 1);
        assert_eq!(json["executables"][1]["summary"]["profiled_samples"], 2);
        assert_eq!(
            json["executables"][0]["artifacts"]["trace"],
            "./_build/native/release/profile/test/ts/main_internal_test/profile.trace"
        );
    }

    fn captured_test_profile(name: &str, parsed: super::ParsedProfile) -> CapturedProfile {
        let output_dir = PathBuf::from(format!("./_build/native/release/profile/test/ts/{name}"));
        CapturedProfile {
            name: name.to_string(),
            output_dir: output_dir.clone(),
            target: profile_target(
                RunMode::Test,
                RunBackend::Native,
                OptLevel::Release,
                Some(PathBuf::from(format!(
                    "./_build/native/release/test/main/{name}.exe"
                ))),
                Vec::new(),
            ),
            artifacts: ProfileArtifacts {
                trace: Some(output_dir.join("profile.trace")),
                time_profile_xml: Some(output_dir.join("time-profile.xml")),
                stdout: Some(output_dir.join("stdout.txt")),
                json_report: None,
            },
            parsed,
        }
    }

    #[test]
    fn sanitizes_dot_only_labels() {
        assert_eq!(sanitize_path_component("."), "profile");
        assert_eq!(sanitize_path_component(".."), "profile");
        assert_eq!(sanitize_path_component("..."), "profile");
        assert_eq!(sanitize_path_component("foo.bar"), "foo.bar");
    }

    #[test]
    fn profile_output_reuses_run_artifact_package_dir() {
        let dir = default_output_dir(
            Path::new("./_build"),
            RunBackend::Native,
            OptLevel::Release,
            RunMode::Run,
            Path::new("./_build/native/release/build/cmd/main/main.exe"),
        );

        assert!(dir.starts_with("./_build/native/release/profile/cmd/main"));
    }

    #[test]
    fn test_profile_session_output_includes_indexed_executable_leaf() {
        let session_dir = Path::new("./_build/native/release/profile/test/20260521-010203");
        let inline = test_profile_output_dir_for_executable(
            session_dir,
            0,
            Path::new("./_build/native/release/test/cmd/main/main_internal_test.exe"),
        );
        let blackbox = test_profile_output_dir_for_executable(
            session_dir,
            1,
            Path::new("./_build/native/release/test/cmd/main/main_blackbox_test.exe"),
        );

        assert_eq!(
            inline,
            PathBuf::from(
                "./_build/native/release/profile/test/20260521-010203/001-main_internal_test"
            )
        );
        assert_eq!(
            blackbox,
            PathBuf::from(
                "./_build/native/release/profile/test/20260521-010203/002-main_blackbox_test"
            )
        );
    }
}
