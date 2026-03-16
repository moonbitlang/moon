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

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use moonbuild_rupes_recta::{intent::UserIntent, model::BuildPlanNode};
use moonutil::{
    cli::UniversalFlags,
    common::{DiagnosticLevel, FileLock, RunMode, TargetBackend},
    mooncakes::sync::AutoSyncFlags,
};
use serde::Deserialize;
use tracing::instrument;

use crate::{
    cli::BuildFlags,
    filter::{
        canonicalize_with_filename, ensure_package_supports_backend, filter_pkg_by_dir,
        package_supports_backend,
    },
    rr_build::{self, BuildConfig, CalcUserIntentOutput, preconfig_compile},
};

/// Prove the current package
#[derive(Debug, clap::Parser, Clone)]
pub(crate) struct ProveSubcommand {
    /// The file-system path to the package or file in package to prove
    #[clap(name = "PATH")]
    pub path: Option<PathBuf>,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Treat all warnings as errors
    #[clap(long, short)]
    pub deny_warn: bool,

    /// Don't render diagnostics (in raw human-readable format)
    #[clap(long)]
    pub no_render: bool,

    /// Output diagnostics in JSON format
    #[clap(long, conflicts_with = "no_render")]
    pub output_json: bool,

    /// Warn list config
    #[clap(long, allow_hyphen_values = true)]
    pub warn_list: Option<String>,

    /// Alert list config
    #[clap(long, allow_hyphen_values = true)]
    pub alert_list: Option<String>,

    /// Set the max number of jobs to run in parallel
    #[clap(short = 'j', long)]
    pub jobs: Option<usize>,

    /// Render no-location diagnostics starting from a certain level
    #[clap(long, value_name = "MIN_LEVEL", default_value = "error")]
    pub render_no_loc: DiagnosticLevel,
}

impl ProveSubcommand {
    fn to_build_flags(&self) -> BuildFlags {
        BuildFlags {
            deny_warn: self.deny_warn,
            no_render: self.no_render,
            output_json: self.output_json,
            warn_list: self.warn_list.clone(),
            alert_list: self.alert_list.clone(),
            jobs: self.jobs,
            render_no_loc: self.render_no_loc,
            ..BuildFlags::default()
        }
    }
}

#[instrument(skip_all)]
pub(crate) fn run_prove(cli: &UniversalFlags, cmd: &ProveSubcommand) -> anyhow::Result<i32> {
    let dirs = cli.source_tgt_dir.try_into_package_dirs()?;
    let source_dir = dirs.source_dir;
    let target_dir = dirs.target_dir;
    let build_flags = cmd.to_build_flags();
    let verif_dir = target_dir.join("verif");
    let why3_config_path = verif_dir.join("why3.conf");

    if !cli.dry_run {
        ensure_why3_config(&why3_config_path)?;
    }

    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &build_flags,
        None,
        &target_dir,
        RunMode::Prove,
    );

    let (build_meta, build_graph) = rr_build::plan_build(
        preconfig,
        &cli.unstable_feature,
        &source_dir,
        &target_dir,
        Box::new(|resolve_output, target_backend| {
            calc_user_intent(cmd.path.as_deref(), resolve_output, target_backend)
        }),
    )?;
    let proof_reports = planned_proof_reports(&build_meta);

    if cli.dry_run {
        rr_build::print_dry_run(
            &build_graph,
            build_meta.artifacts.values(),
            &source_dir,
            &target_dir,
        );
        return Ok(0);
    }

    let _lock = FileLock::lock(&target_dir)?;
    rr_build::generate_all_pkgs_json(&target_dir, &build_meta, RunMode::Prove)?;
    let cfg = BuildConfig::from_flags(&build_flags, &cli.unstable_feature, cli.verbose);
    let result = rr_build::execute_build(&cfg, build_graph, &target_dir)?;
    if !cli.quiet && !build_flags.output_json {
        let _ = print_prove_summary(&source_dir, &proof_reports);
    }
    Ok(result.return_code_for_success())
}

fn calc_user_intent(
    path_filter: Option<&Path>,
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    target_backend: TargetBackend,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let &[main_module_id] = resolve_output.local_modules() else {
        panic!("No multiple main modules are supported");
    };

    if let Some(path) = path_filter {
        let (dir, _) = canonicalize_with_filename(path)?;
        let pkg = filter_pkg_by_dir(resolve_output, &dir)?;
        ensure_package_supports_backend(resolve_output, pkg, target_backend)?;
        Ok(vec![UserIntent::Prove(pkg)].into())
    } else {
        let packages = resolve_output
            .pkg_dirs
            .packages_for_module(main_module_id)
            .ok_or_else(|| anyhow::anyhow!("Cannot find the local module!"))?;
        let intents = packages
            .values()
            .copied()
            .filter(|&pkg| package_supports_backend(resolve_output, pkg, target_backend))
            .filter(|&pkg| {
                resolve_output
                    .pkg_dirs
                    .get_package(pkg)
                    .has_implementation()
            })
            .map(UserIntent::Prove)
            .collect::<Vec<_>>();
        Ok(intents.into())
    }
}

fn ensure_why3_config(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }

    let z3_path = resolve_z3_path()?;
    let z3_path = z3_path.to_string_lossy().into_owned();
    let version = detect_z3_version(&z3_path)?;

    let parent = path
        .parent()
        .context("why3 config path must have a parent directory")?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create verification output directory `{}`",
            parent.display()
        )
    })?;

    let config = format!(
        r#"[main]
magic = 14
memlimit = 1000
running_provers_max = 16
timelimit = 5.000000

[partial_prover]
name = "Z3"
path = "{z3_path}"
version = "{version}"

[strategy]
code = "start:
c Z3,{version} .2 1000
c Z3,{version} 1 1000
t compute_specified start
t split_vc start
c Z3,{version} 2 4000
"
desc = "Automatic@ run@ of@ Z3@ and@ most@ useful@ transformations"
name = "MoonBit_Auto"
shortcut = "4"
"#,
        z3_path = z3_path,
        version = version,
    );

    std::fs::write(path, config)
        .with_context(|| format!("failed to write why3 config to `{}`", path.display()))?;
    Ok(())
}

fn resolve_z3_path() -> anyhow::Result<PathBuf> {
    if let Ok(path) = which::which("z3") {
        return Ok(path);
    }

    if let Some(path) = std::env::var_os("Z3PATH").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }

    bail!(
        "failed to locate z3 for `moon prove`: searched for `z3` in PATH and `Z3PATH` is not set. Install `z3` or set `Z3PATH=/path/to/z3`"
    );
}

fn detect_z3_version(z3_path: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new(z3_path)
        .arg("--version")
        .output()
        .with_context(|| format!("failed to execute `{z3_path} --version`"))?;
    if !output.status.success() {
        bail!("`{z3_path} --version` exited with status {}", output.status);
    }

    let stdout = String::from_utf8(output.stdout)
        .with_context(|| format!("`{z3_path} --version` did not emit valid UTF-8"))?;
    let version =
        parse_z3_version(&stdout).context("failed to parse Z3 version from `--version` output")?;
    Ok(version.to_string())
}

fn parse_z3_version(stdout: &str) -> Option<&str> {
    stdout
        .split_whitespace()
        .map(|token| token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.'))
        .find(|token| {
            !token.is_empty()
                && token.chars().next().is_some_and(|c| c.is_ascii_digit())
                && token.chars().all(|c| c.is_ascii_digit() || c == '.')
        })
}

#[derive(Debug)]
struct PlannedProofReport {
    package: String,
    whyml_path: PathBuf,
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct ProofReport {
    result: String,
    summary: ProofReportSummary,
    #[serde(default)]
    failures: Vec<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct ProofReportSummary {
    valid: u64,
    invalid: u64,
    timeout: u64,
    oom: u64,
    step_limit: u64,
    unknown: u64,
    failure: u64,
}

impl ProofReportSummary {
    fn add_assign(&mut self, other: &Self) {
        self.valid += other.valid;
        self.invalid += other.invalid;
        self.timeout += other.timeout;
        self.oom += other.oom;
        self.step_limit += other.step_limit;
        self.unknown += other.unknown;
        self.failure += other.failure;
    }
}

fn planned_proof_reports(build_meta: &rr_build::BuildMeta) -> Vec<PlannedProofReport> {
    let mut reports = build_meta
        .artifacts
        .values()
        .filter_map(|artifacts| match artifacts.node {
            BuildPlanNode::Prove(target) => {
                let path = artifacts
                    .artifacts
                    .iter()
                    .find(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.ends_with(".proof.json"))
                    })?
                    .clone();
                let whyml_path = artifacts
                    .artifacts
                    .iter()
                    .find(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.ends_with(".mlw"))
                    })?
                    .clone();
                let package = build_meta
                    .resolve_output
                    .pkg_dirs
                    .get_package(target.package)
                    .fqn
                    .to_string();
                Some(PlannedProofReport {
                    package,
                    whyml_path,
                    path,
                })
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    reports.sort_by(|a, b| a.package.cmp(&b.package));
    reports
}

fn print_prove_summary(
    workspace_root: &Path,
    proof_reports: &[PlannedProofReport],
) -> anyhow::Result<()> {
    if proof_reports.is_empty() {
        return Ok(());
    }

    let mut package_reports = Vec::with_capacity(proof_reports.len());
    for proof_report in proof_reports {
        let content = match std::fs::read_to_string(&proof_report.path) {
            Ok(content) => content,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "failed to read proof report for `{}` from `{}`",
                        proof_report.package,
                        proof_report.path.display()
                    )
                });
            }
        };
        let report: ProofReport = serde_json::from_str(&content).with_context(|| {
            format!(
                "failed to parse proof report for `{}` from `{}`",
                proof_report.package,
                proof_report.path.display()
            )
        })?;
        package_reports.push(PackageProofSummary {
            package: proof_report.package.as_str(),
            whyml_path: &proof_report.whyml_path,
            report_path: &proof_report.path,
            report,
        });
    }

    if package_reports.is_empty() {
        return Ok(());
    }

    for line in format_prove_summary(workspace_root, &package_reports) {
        println!("{line}");
    }
    Ok(())
}

#[derive(Debug)]
struct PackageProofSummary<'a> {
    package: &'a str,
    whyml_path: &'a Path,
    report_path: &'a Path,
    report: ProofReport,
}

fn format_prove_summary(
    workspace_root: &Path,
    package_reports: &[PackageProofSummary<'_>],
) -> Vec<String> {
    if package_reports.is_empty() {
        return Vec::new();
    }

    let mut total = ProofReportSummary::default();
    let mut succeeded = 0usize;

    for summary in package_reports {
        total.add_assign(&summary.report.summary);
        if summary.report.result == "success" {
            succeeded += 1;
        }
    }

    let mut lines = Vec::new();
    for (index, summary) in package_reports.iter().enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
        lines.push(summary.package.to_string());
        if summary.report.result == "success" {
            lines.push(format!(
                "  Succeeded: {} goals proved",
                summary.report.summary.valid
            ));
        } else {
            lines.push(format!(
                "  Failed: {}",
                format_goal_counts(&summary.report.summary)
            ));
            lines.push(format!(
                "  WhyML: {}",
                display_path(workspace_root, summary.whyml_path)
            ));
            lines.push(format!(
                "  Report: {}",
                display_path(workspace_root, summary.report_path)
            ));
            if !summary.report.failures.is_empty() {
                lines.push(format!("  Failed goals: {}", summary.report.failures.len()));
            }
        }
    }

    lines.push(String::new());
    lines.push("Summary:".to_string());
    lines.push(format!(
        "  {} of {} packages proved",
        succeeded,
        package_reports.len()
    ));
    lines.push(format!("  {}", format_goal_counts(&total)));
    lines
}

fn format_goal_counts(summary: &ProofReportSummary) -> String {
    let mut parts = vec![format!("{} goals proved", summary.valid)];
    if summary.invalid > 0 {
        parts.push(format!("{} invalid", summary.invalid));
    }
    if summary.timeout > 0 {
        parts.push(format!("{} timeout", summary.timeout));
    }
    if summary.oom > 0 {
        parts.push(format!("{} oom", summary.oom));
    }
    if summary.step_limit > 0 {
        parts.push(format!("{} step_limit", summary.step_limit));
    }
    if summary.unknown > 0 {
        parts.push(format!("{} unknown", summary.unknown));
    }
    if summary.failure > 0 {
        parts.push(format!("{} failure", summary.failure));
    }
    parts.join(", ")
}

fn display_path(workspace_root: &Path, path: &Path) -> String {
    path.strip_prefix(workspace_root)
        .unwrap_or(path)
        .display()
        .to_string()
}
