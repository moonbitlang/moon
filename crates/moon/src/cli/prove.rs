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
    mooncakes::{ModuleId, sync::AutoSyncFlags},
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
    user_diagnostics::UserDiagnostics,
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
            jobs: self.jobs,
            render_no_loc: self.render_no_loc,
            ..BuildFlags::default()
        }
    }
}

#[instrument(skip_all)]
pub(crate) fn run_prove(cli: &UniversalFlags, cmd: &ProveSubcommand) -> anyhow::Result<i32> {
    let dirs = cli.source_tgt_dir.try_into_workspace_module_dirs()?;
    let module_dir = if cmd.path.is_some() {
        dirs.module_dir.clone()
    } else {
        Some(dirs.require_module_dir("prove")?.clone())
    };
    let project_root = dirs.project_root;
    let target_dir = dirs.target_dir;
    let project_manifest_path = dirs.project_manifest_path;
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
    let path_filter = cmd.path.as_deref();

    let (build_meta, build_graph) = rr_build::plan_build(
        preconfig,
        &cli.unstable_feature,
        &project_root,
        &target_dir,
        UserDiagnostics::from_flags(cli),
        Some(project_manifest_path.as_path()),
        Box::new(move |resolve_output, target_backend| {
            calc_user_intent(
                path_filter,
                module_dir.as_deref(),
                resolve_output,
                target_backend,
            )
        }),
    )?;
    let proof_reports = planned_proof_reports(&build_meta);

    if cli.dry_run {
        rr_build::print_dry_run(
            &build_graph,
            build_meta.artifacts.values(),
            &project_root,
            &target_dir,
        );
        return Ok(0);
    }

    let _lock = FileLock::lock(&target_dir)?;
    rr_build::generate_all_pkgs_json(&target_dir, &build_meta, RunMode::Prove)?;
    let cfg = BuildConfig::from_flags(
        &build_flags,
        &cli.unstable_feature,
        cli.verbose,
        UserDiagnostics::from_flags(cli),
    );
    let result = rr_build::execute_build(&cfg, build_graph, &target_dir)?;
    if !cli.quiet && !build_flags.output_json {
        let _ = print_prove_summary(&project_root, &proof_reports);
    }
    Ok(result.return_code_for_success())
}

fn calc_user_intent(
    path_filter: Option<&Path>,
    selected_module_dir: Option<&Path>,
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    target_backend: TargetBackend,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    if let Some(path) = path_filter {
        let (dir, _) = canonicalize_with_filename(path)?;
        let pkg = filter_pkg_by_dir(resolve_output, &dir)?;
        ensure_package_supports_backend(resolve_output, pkg, target_backend)?;
        Ok(vec![UserIntent::Prove(pkg)].into())
    } else {
        let main_module_id = selected_main_module_id(resolve_output, selected_module_dir)?;
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

fn selected_main_module_id(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    selected_module_dir: Option<&Path>,
) -> anyhow::Result<ModuleId> {
    if let Some(selected_module_dir) = selected_module_dir {
        return resolve_output
            .local_modules()
            .iter()
            .copied()
            .find(|&module_id| {
                resolve_output
                    .module_dirs
                    .get(module_id)
                    .is_some_and(|module_dir| module_dir == selected_module_dir)
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Cannot find the local module at `{}`",
                    selected_module_dir.display()
                )
            });
    }

    match resolve_output.local_modules() {
        &[main_module_id] => Ok(main_module_id),
        _ => bail!("No multiple main modules are supported"),
    }
}

struct SolverSpec {
    name: &'static str,
    binary: &'static str,
    env_var: &'static str,
}

/// Supported solvers in alphabetical order (controls `[partial_prover]` section ordering).
const SOLVER_SPECS: &[SolverSpec] = &[
    SolverSpec {
        name: "Alt-Ergo",
        binary: "alt-ergo",
        env_var: "ALTERGOPATH",
    },
    SolverSpec {
        name: "CVC4",
        binary: "cvc4",
        env_var: "CVC4PATH",
    },
    SolverSpec {
        name: "CVC5",
        binary: "cvc5",
        env_var: "CVC5PATH",
    },
    SolverSpec {
        name: "Z3",
        binary: "z3",
        env_var: "Z3PATH",
    },
];

/// Priority order for strategy lines (differs from alphabetical).
const STRATEGY_ORDER: &[&str] = &["Alt-Ergo", "Z3", "CVC5", "CVC4"];

#[derive(Debug)]
struct DetectedSolver {
    name: &'static str,
    path: String,
    version: String,
}

fn ensure_why3_config(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }

    let solvers = detect_solvers()?;

    let parent = path
        .parent()
        .context("why3 config path must have a parent directory")?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create verification output directory `{}`",
            parent.display()
        )
    })?;

    let config = generate_why3_config(&solvers);

    std::fs::write(path, config)
        .with_context(|| format!("failed to write why3 config to `{}`", path.display()))?;
    Ok(())
}

fn detect_solvers() -> anyhow::Result<Vec<DetectedSolver>> {
    let solvers: Vec<DetectedSolver> = SOLVER_SPECS.iter().filter_map(try_detect_solver).collect();

    if solvers.is_empty() {
        bail!(
            "failed to locate any SMT solver for `moon prove`: \
             searched for `alt-ergo`, `cvc4`, `cvc5`, `z3` in PATH. \
             You can also set ALTERGOPATH, CVC4PATH, CVC5PATH, or Z3PATH. \
             Install at least one of: Alt-Ergo, CVC4, CVC5, Z3."
        );
    }

    Ok(solvers)
}

fn try_detect_solver(spec: &SolverSpec) -> Option<DetectedSolver> {
    let path = which::which(spec.binary).ok().or_else(|| {
        std::env::var_os(spec.env_var)
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
    })?;
    let path_str = path.to_string_lossy().into_owned();

    let output = std::process::Command::new(&path)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        tracing::warn!(
            "{} at `{}`: `--version` exited with status {}",
            spec.name,
            path_str,
            output.status
        );
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let version = parse_solver_version(&stdout)?;

    Some(DetectedSolver {
        name: spec.name,
        path: path_str,
        version: version.to_string(),
    })
}

fn generate_why3_config(solvers: &[DetectedSolver]) -> String {
    let mut config = String::from(
        "[main]\n\
         magic = 14\n\
         memlimit = 1000\n\
         running_provers_max = 16\n\
         timelimit = 5.000000\n",
    );

    for solver in solvers {
        config.push_str(&format!(
            "\n[partial_prover]\n\
             name = \"{}\"\n\
             path = \"{}\"\n\
             version = \"{}\"\n",
            solver.name, solver.path, solver.version,
        ));
    }

    let strategy = generate_strategy(solvers);
    config.push_str(&format!(
        "\n[strategy]\n\
         code = \"start:\n\
         {strategy}\
         \"\n\
         desc = \"Automatic@ run@ of@ provers@ and@ most@ useful@ transformations\"\n\
         name = \"MoonBit_Auto\"\n\
         shortcut = \"4\"\n",
    ));

    config
}

fn generate_strategy(solvers: &[DetectedSolver]) -> String {
    // Order solvers by STRATEGY_ORDER, filtering to those actually detected.
    let ordered: Vec<&DetectedSolver> = STRATEGY_ORDER
        .iter()
        .filter_map(|&name| solvers.iter().find(|s| s.name == name))
        .collect();

    let solver_ref = |s: &DetectedSolver| format!("{},{}", s.name, s.version);
    let mut strategy = String::new();

    // Stage 1: Quick sequential attempts (0.2s, 1000 steps)
    for s in &ordered {
        strategy.push_str(&format!("c {} .2 1000\n", solver_ref(s)));
    }

    // Stage 2: Parallel medium attempts (1s, 1000 steps)
    let medium: Vec<String> = ordered
        .iter()
        .map(|s| format!("c {} 1 1000", solver_ref(s)))
        .collect();
    strategy.push_str(&medium.join(" | "));
    strategy.push('\n');

    // Stage 3: Transformations
    strategy.push_str("t compute_specified start\n");
    strategy.push_str("t split_vc start\n");

    // Stage 4: Parallel long attempts (2s, 4000 steps)
    let long: Vec<String> = ordered
        .iter()
        .map(|s| format!("c {} 2 4000", solver_ref(s)))
        .collect();
    strategy.push_str(&long.join(" | "));
    strategy.push('\n');

    strategy
}

fn parse_solver_version(stdout: &str) -> Option<&str> {
    stdout
        .split_whitespace()
        .map(|token| token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.'))
        .map(|token| {
            token
                .strip_prefix('v')
                .or(token.strip_prefix('V'))
                .unwrap_or(token)
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_solver_version_z3() {
        assert_eq!(
            parse_solver_version("Z3 version 4.15.3 - 64 bit"),
            Some("4.15.3")
        );
    }

    #[test]
    fn test_parse_solver_version_alt_ergo() {
        assert_eq!(parse_solver_version("v2.6.2"), Some("2.6.2"));
    }

    #[test]
    fn test_parse_solver_version_cvc4() {
        assert_eq!(
            parse_solver_version("This is CVC4 version 1.8\ncompiled with GCC"),
            Some("1.8")
        );
    }

    #[test]
    fn test_parse_solver_version_cvc5() {
        assert_eq!(
            parse_solver_version("This is cvc5 version 1.3.1\ncompiled with GCC"),
            Some("1.3.1")
        );
    }

    #[test]
    fn test_parse_solver_version_empty() {
        assert_eq!(parse_solver_version(""), None);
    }

    #[test]
    fn test_parse_solver_version_no_version() {
        assert_eq!(parse_solver_version("no version here"), None);
    }

    fn make_solver(name: &'static str, path: &str, version: &str) -> DetectedSolver {
        DetectedSolver {
            name,
            path: path.to_string(),
            version: version.to_string(),
        }
    }

    #[test]
    fn test_generate_strategy_single_solver() {
        let solvers = vec![make_solver("Z3", "/usr/bin/z3", "4.15.3")];
        let strategy = generate_strategy(&solvers);
        assert_eq!(
            strategy,
            "c Z3,4.15.3 .2 1000\n\
             c Z3,4.15.3 1 1000\n\
             t compute_specified start\n\
             t split_vc start\n\
             c Z3,4.15.3 2 4000\n"
        );
    }

    #[test]
    fn test_generate_strategy_two_solvers() {
        let solvers = vec![
            make_solver("CVC5", "/usr/bin/cvc5", "1.3.1"),
            make_solver("Z3", "/usr/bin/z3", "4.15.3"),
        ];
        let strategy = generate_strategy(&solvers);
        assert_eq!(
            strategy,
            "c Z3,4.15.3 .2 1000\n\
             c CVC5,1.3.1 .2 1000\n\
             c Z3,4.15.3 1 1000 | c CVC5,1.3.1 1 1000\n\
             t compute_specified start\n\
             t split_vc start\n\
             c Z3,4.15.3 2 4000 | c CVC5,1.3.1 2 4000\n"
        );
    }

    #[test]
    fn test_generate_strategy_all_solvers() {
        let solvers = vec![
            make_solver("Alt-Ergo", "/usr/bin/alt-ergo", "2.6.2"),
            make_solver("CVC4", "/usr/bin/cvc4", "1.8"),
            make_solver("CVC5", "/usr/bin/cvc5", "1.3.1"),
            make_solver("Z3", "/usr/bin/z3", "4.15.3"),
        ];
        let strategy = generate_strategy(&solvers);
        assert_eq!(
            strategy,
            "c Alt-Ergo,2.6.2 .2 1000\n\
             c Z3,4.15.3 .2 1000\n\
             c CVC5,1.3.1 .2 1000\n\
             c CVC4,1.8 .2 1000\n\
             c Alt-Ergo,2.6.2 1 1000 | c Z3,4.15.3 1 1000 | c CVC5,1.3.1 1 1000 | c CVC4,1.8 1 1000\n\
             t compute_specified start\n\
             t split_vc start\n\
             c Alt-Ergo,2.6.2 2 4000 | c Z3,4.15.3 2 4000 | c CVC5,1.3.1 2 4000 | c CVC4,1.8 2 4000\n"
        );
    }

    #[test]
    fn test_generate_config_single_solver() {
        let solvers = vec![make_solver("Z3", "/usr/bin/z3", "4.15.3")];
        let config = generate_why3_config(&solvers);
        assert!(config.contains("[partial_prover]\nname = \"Z3\""));
        assert!(config.contains("name = \"MoonBit_Auto\""));
        assert!(!config.contains("Alt-Ergo"));
    }

    #[test]
    fn test_generate_config_all_solvers() {
        let solvers = vec![
            make_solver("Alt-Ergo", "/usr/bin/alt-ergo", "2.6.2"),
            make_solver("CVC4", "/usr/bin/cvc4", "1.8"),
            make_solver("CVC5", "/usr/bin/cvc5", "1.3.1"),
            make_solver("Z3", "/usr/bin/z3", "4.15.3"),
        ];
        let config = generate_why3_config(&solvers);
        // All partial_prover sections present in alphabetical order
        let alt_ergo_pos = config.find("name = \"Alt-Ergo\"").unwrap();
        let cvc4_pos = config.find("name = \"CVC4\"").unwrap();
        let cvc5_pos = config.find("name = \"CVC5\"").unwrap();
        let z3_pos = config.find("name = \"Z3\"").unwrap();
        assert!(alt_ergo_pos < cvc4_pos);
        assert!(cvc4_pos < cvc5_pos);
        assert!(cvc5_pos < z3_pos);
    }
}
