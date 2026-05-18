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

#[derive(Debug, Clone, Serialize)]
struct ProfileReport {
    schema: u32,
    backend: String,
    executable: PathBuf,
    trace_path: PathBuf,
    time_profile_xml: PathBuf,
    target_stdout: PathBuf,
    total_samples: usize,
    sample_weight_ms: f64,
    top_self: Vec<ProfileEntry>,
    top_inclusive: Vec<ProfileEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileEntry {
    symbol: String,
    mangled_symbol: String,
    samples: usize,
    time_ms: f64,
    percent: f64,
}

#[derive(Debug, Default)]
struct ParsedProfile {
    total_samples: usize,
    sample_weight_ms: f64,
    self_counts: HashMap<String, usize>,
    self_ms: HashMap<String, f64>,
    inclusive_counts: HashMap<String, usize>,
    inclusive_ms: HashMap<String, f64>,
}

pub(crate) fn run_profiled_run(cli: &UniversalFlags, cmd: RunSubcommand) -> anyhow::Result<i32> {
    if !cfg!(target_os = "macos") {
        bail!("`moon run --profile` currently supports macOS only");
    }

    ensure_xctrace_available()?;

    if cmd.command.is_some() {
        bail!("`moon run --profile` does not support inline `-c` source");
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
        &built.executable,
    );

    let trace_path = output_dir.join("profile.trace");
    let xml_path = output_dir.join("time-profile.xml");
    let report_path = output_dir.join("report.txt");
    let json_path = output_dir.join("profile.json");
    let stdout_path = output_dir.join("stdout.txt");

    if cli.dry_run {
        print_xctrace_record_command(&trace_path, &stdout_path, &built.executable, &cmd.args);
        print_xctrace_export_command(&trace_path, &xml_path);
        return Ok(0);
    }

    built.release_lock();

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

    run_xctrace_record(&trace_path, &stdout_path, &built.executable, &cmd.args)?;
    run_xctrace_export(&trace_path, &xml_path)?;

    let parsed = parse_xctrace_time_profile(&xml_path)?;
    let report = build_report(
        parsed,
        built.executable,
        trace_path,
        xml_path,
        stdout_path,
        DEFAULT_TOP,
    );
    let terminal_report = render_terminal_report(&report, &report_path, &json_path);
    print!("{terminal_report}");
    std::fs::write(&report_path, &terminal_report)
        .with_context(|| format!("failed to write profile report `{}`", report_path.display()))?;
    std::fs::write(&json_path, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write profile json `{}`", json_path.display()))?;

    Ok(0)
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

fn default_output_dir(
    target_dir: &Path,
    target_backend: RunBackend,
    opt_level: OptLevel,
    executable: &Path,
) -> PathBuf {
    let timestamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
    let build_root = output_kind_dir(
        target_dir,
        target_backend,
        opt_level,
        RunMode::Run.to_dir_name(),
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
    profile_dir.join(timestamp)
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

fn ensure_xctrace_available() -> anyhow::Result<()> {
    let output = Command::new("xcrun")
        .args(["xctrace", "version"])
        .output()
        .context("failed to execute `xcrun xctrace version`")?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!(
        "`moon run --profile` on macOS requires `xcrun xctrace`.\n\n\
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
            continue;
        }

        let weight = first_tag(row, "weight")
            .and_then(|tag| resolve_value(tag, &weights, ""))
            .unwrap_or(parsed.sample_weight_ms);

        let Some(stack) = stack else { continue };
        let stack = stack
            .into_iter()
            .filter(|name| is_profile_symbol(name))
            .collect::<Vec<_>>();
        if stack.is_empty() {
            continue;
        }

        parsed.total_samples += 1;
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
    executable: PathBuf,
    trace_path: PathBuf,
    xml_path: PathBuf,
    stdout_path: PathBuf,
    limit: usize,
) -> ProfileReport {
    let total = parsed.total_samples.max(1) as f64;
    let top_self = top_entries(parsed.self_counts, parsed.self_ms, total, limit, |_| true);
    let top_inclusive = top_entries(
        parsed.inclusive_counts,
        parsed.inclusive_ms,
        total,
        limit,
        |symbol| !skip_inclusive_symbol(symbol),
    );
    ProfileReport {
        schema: 1,
        backend: "xctrace".to_string(),
        executable,
        trace_path,
        time_profile_xml: xml_path,
        target_stdout: stdout_path,
        total_samples: parsed.total_samples,
        sample_weight_ms: parsed.sample_weight_ms,
        top_self,
        top_inclusive,
    }
}

fn top_entries(
    counts: HashMap<String, usize>,
    times: HashMap<String, f64>,
    total_samples: f64,
    limit: usize,
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
                symbol,
                mangled_symbol,
                samples,
                time_ms,
                percent: samples as f64 * 100.0 / total_samples,
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        b.samples
            .cmp(&a.samples)
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    entries.truncate(limit);
    entries
}

fn demangle_symbol(symbol: &str) -> String {
    if symbol.starts_with("_M0") {
        moonutil::demangle::demangle_mangled_function_name(symbol)
    } else {
        symbol.to_string()
    }
}

fn skip_inclusive_symbol(symbol: &str) -> bool {
    let demangled = demangle_symbol(symbol);
    demangled == "main"
        || demangled.contains("____moonbit__main")
        || demangled.starts_with("@moonbitlang/async/")
}

fn render_terminal_report(report: &ProfileReport, report_path: &Path, json_path: &Path) -> String {
    let mut out = String::new();
    out.push_str("Profile: native macOS Time Profiler\n");
    out.push_str(&format!(
        "Samples: {}, sample weight: {:.2}ms\n",
        report.total_samples, report.sample_weight_ms
    ));
    out.push_str(&format!("Trace: {}\n", report.trace_path.display()));
    out.push_str(&format!(
        "Program stdout: {}\n",
        report.target_stdout.display()
    ));
    out.push('\n');
    out.push_str("Top self time:\n");
    render_entries(&mut out, &report.top_self);
    out.push('\n');
    out.push_str("Top inclusive time:\n");
    render_entries(&mut out, &report.top_inclusive);
    out.push('\n');
    out.push_str(&format!("Full text report: {}\n", report_path.display()));
    out.push_str(&format!(
        "Machine-readable report: {}\n",
        json_path.display()
    ));
    out.push_str("Open the full trace in Instruments:\n");
    out.push_str(&format!("  open {}\n", report.trace_path.display()));
    out
}

fn render_entries(out: &mut String, entries: &[ProfileEntry]) {
    if entries.is_empty() {
        out.push_str("  no samples\n");
        return;
    }
    for entry in entries {
        out.push_str(&format!(
            "  {:>5.1}% {:>8.2}ms {:>6}  {}\n",
            entry.percent, entry.time_ms, entry.samples, entry.symbol
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use moonbuild_rupes_recta::model::RunBackend;
    use moonutil::cond_expr::OptLevel;

    use super::{default_output_dir, parse_xctrace_time_profile_xml, sanitize_path_component};

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
            Path::new("./_build/native/release/build/cmd/main/main.exe"),
        );

        assert!(dir.starts_with("./_build/native/release/profile/cmd/main"));
    }
}
