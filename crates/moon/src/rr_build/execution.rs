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

//! Execute a concrete plan and process captured diagnostics.
//!
//! The private `n2` module owns graph adaptation, scheduling, and the database.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use ariadne::ReportKind;
use clap::ValueEnum;
use colored::Colorize;
use moonbuild_rupes_recta::target_layout::GENERATED_TEST_DRIVER_PREFIX;
use moonutil::{
    features::FeatureGate, render::MooncDiagnostic, target::TargetBackend,
    test_metadata::DiagnosticLevel, user_log::UserLog,
};
use tracing::instrument;

use super::{BuildInput, BuildMeta};
use crate::build_flags::{BuildFlags, OutputStyle};

mod n2;

/// Execution and diagnostic options. Planning inputs live in `BuildInput`.
#[derive(Clone)]
pub(crate) struct BuildConfig {
    /// The level of parallelism to use. If `None`, will use the number of
    /// available CPU cores.
    parallelism: Option<usize>,
    /// The output style for errors and warnings
    output_style: OutputStyle,
    /// Render no-location diagnostics above this level
    render_no_loc: DiagnosticLevel,
    /// Maximum number of diagnostics to display after deduplication.
    diagnostic_limit: Option<usize>,

    /// Explain and warnings in diagnostics
    pub explain_errors: bool,

    /// Ask n2 to explain rerun reasons
    n2_explain: bool,

    /// Verbose output for build progress and command echo
    verbose: bool,
    suppress_progress: bool,

    /// The patch file to use
    pub patch_file: Option<PathBuf>,
}

impl BuildConfig {
    pub(crate) fn from_flags(
        flags: &BuildFlags,
        unstable_features: &FeatureGate,
        verbose: bool,
    ) -> Self {
        BuildConfig {
            parallelism: flags.jobs,
            output_style: flags.output_style(),
            render_no_loc: flags.render_no_loc,
            diagnostic_limit: flags.diagnostic_limit,
            explain_errors: false,
            n2_explain: unstable_features.rr_n2_explain,
            verbose,
            suppress_progress: false,
            patch_file: None,
        }
    }

    pub(crate) fn with_suppressed_progress(mut self, suppress_progress: bool) -> Self {
        self.suppress_progress = suppress_progress;
        self
    }
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            parallelism: None,
            output_style: OutputStyle::Raw,
            render_no_loc: DiagnosticLevel::Error,
            diagnostic_limit: None,
            explain_errors: false,
            n2_explain: false,
            verbose: false,
            suppress_progress: false,
            patch_file: None,
        }
    }
}

#[derive(Default)]
struct ResultCatcher {
    content_writer: Vec<String>,
    n_warnings: usize,
    n_errors: usize,
}

impl ResultCatcher {
    fn append_content(&mut self, s: impl Into<String>) {
        self.content_writer.push(s.into());
    }

    fn append_kind(&mut self, kind: Option<ReportKind>) {
        match kind {
            Some(ReportKind::Error) => self.n_errors += 1,
            Some(ReportKind::Warning) => self.n_warnings += 1,
            _ => {}
        }
    }

    fn append_diag(&mut self, diag: &moonutil::render::MooncDiagnostic) {
        if diag.level == "error" {
            self.n_errors += 1;
        } else if diag.level == "warning" {
            self.n_warnings += 1;
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct BuildStats {
    /// Number of build tasks executed, `None` means build failure
    n_tasks_executed: Option<usize>,

    n_errors: usize,
    n_warnings: usize,
}

impl BuildStats {
    /// Whether the run was successful (i.e. didn't fail to execute).
    pub(crate) fn successful(&self) -> bool {
        self.n_tasks_executed.is_some()
    }

    /// Get the return code that should be returned to the shell.
    pub(crate) fn return_code_for_success(&self) -> i32 {
        if self.successful() { 0 } else { 1 }
    }

    pub(crate) fn print_info(&self, quiet: bool, mode: &str) -> anyhow::Result<()> {
        match self.n_tasks_executed {
            None => {
                eprintln!(
                    "Failed with {} warnings, {} errors.",
                    self.n_warnings, self.n_errors
                );
                anyhow::bail!("failed when {mode} project");
            }
            Some(n_tasks) => {
                if !quiet {
                    let finished = "Finished.".green().bold();
                    let warnings_errors = if self.n_warnings > 0 || self.n_errors > 0 {
                        format!(" ({} warnings, {} errors)", self.n_warnings, self.n_errors)
                    } else {
                        String::new()
                    };

                    match n_tasks {
                        0 => {
                            eprintln!("{finished} moon: no work to do{warnings_errors}");
                        }
                        n => {
                            let task_plural = if n == 1 { "" } else { "s" };
                            eprintln!(
                                "{finished} moon: ran {n} task{task_plural}, now up to date{warnings_errors}"
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

struct CapturedBuildExecution {
    n_tasks_executed: Option<usize>,
    action_outputs: Vec<CapturedActionOutput>,
}

struct CapturedActionOutput {
    target_backend: Option<TargetBackend>,
    content: ResultCatcher,
}

impl CapturedBuildExecution {
    fn successful(&self) -> bool {
        self.n_tasks_executed.is_some()
    }

    fn diagnostic_sources<'a>(
        &'a self,
        build_metas: impl IntoIterator<Item = &'a BuildMeta>,
    ) -> Vec<CapturedDiagnosticSource<'a>> {
        let build_metas = build_metas.into_iter().collect::<Vec<_>>();
        let sole_build_meta = match build_metas.as_slice() {
            [build_meta] => Some(*build_meta),
            _ => None,
        };
        self.action_outputs
            .iter()
            .map(|output| {
                let build_meta = output
                    .target_backend
                    .and_then(|backend| {
                        build_metas
                            .iter()
                            .find(|meta| meta.target_backend() == backend)
                            .copied()
                    })
                    .or(sole_build_meta);
                CapturedDiagnosticSource {
                    diagnostics: &output.content,
                    build_succeeded: self.successful(),
                    build_meta,
                }
            })
            .collect()
    }
}

/// Execute a build plan.
///
/// Takes ownership of the build graph and executes the actual build tasks.
/// Returns just the build result - callers should use the resolve data and
/// artifacts from the planning phase for any metadata they need.
///
/// The caller must hold the target-directory lock. All executions in
/// that directory share one n2 database, and n2 does not lock it internally.
#[instrument(skip_all)]
pub(crate) fn execute_build(
    cfg: &BuildConfig,
    input: BuildInput,
    target_dir: &Path,
    user_log: &UserLog,
) -> anyhow::Result<BuildStats> {
    let execution = execute_build_capturing(cfg, input, target_dir)?;
    Ok(finish_captured_build(cfg, &execution, None, user_log))
}

/// Structured output from one build execution for a command-level JSON
/// renderer. The executor does not write diagnostics or summaries itself.
pub(crate) struct JsonBuildOutput {
    pub n_tasks_executed: Option<usize>,
    pub n_errors: usize,
    pub n_warnings: usize,
    pub hidden_errors: usize,
    pub hidden_warnings: usize,
    pub diagnostics: Vec<JsonBuildDiagnostic>,
    pub non_diagnostic_output: Vec<String>,
}

/// One compiler diagnostic and the backend of the action that emitted it.
/// The command layer remains responsible for projecting this into its JSON
/// schema.
pub(crate) struct JsonBuildDiagnostic {
    pub target_backend: Option<TargetBackend>,
    pub value: serde_json::Value,
}

impl JsonBuildOutput {
    pub(crate) fn successful(&self) -> bool {
        self.n_tasks_executed.is_some()
    }
}

/// Execute a build while returning all Moonc diagnostics to the CLI seam.
pub(crate) fn execute_build_json(
    cfg: &BuildConfig,
    input: BuildInput,
    target_dir: &Path,
) -> anyhow::Result<JsonBuildOutput> {
    let execution = execute_build_capturing(cfg, input, target_dir)?;
    // Keep the existing per-backend diagnostic-limit semantics while all
    // backends execute in one n2 graph. Shared actions have no backend and are
    // collected in their own group.
    let mut sources_by_backend = BTreeMap::new();
    for output in &execution.action_outputs {
        sources_by_backend
            .entry(output.target_backend)
            .or_insert_with(Vec::new)
            .push(CapturedDiagnosticSource {
                diagnostics: &output.content,
                build_succeeded: execution.successful(),
                build_meta: None,
            });
    }

    let mut n_errors = 0;
    let mut n_warnings = 0;
    let mut hidden_errors = 0;
    let mut hidden_warnings = 0;
    let mut diagnostics = Vec::new();
    let mut non_diagnostic_output = Vec::new();
    for (target_backend, sources) in sources_by_backend {
        let collected = collect_json_diagnostics(&sources, cfg, true);
        n_errors += collected.processed.n_errors;
        n_warnings += collected.processed.n_warnings;
        hidden_errors += collected.processed.hidden_errors;
        hidden_warnings += collected.processed.hidden_warnings;
        diagnostics.extend(collected.diagnostics.into_iter().map(|content| {
            JsonBuildDiagnostic {
                target_backend,
                value: serde_json::from_str(&content)
                    .expect("collected Moonc diagnostic should remain valid JSON"),
            }
        }));
        non_diagnostic_output.extend(collected.non_diagnostic_output);
    }

    Ok(JsonBuildOutput {
        n_tasks_executed: execution.n_tasks_executed,
        n_errors,
        n_warnings,
        hidden_errors,
        hidden_warnings,
        diagnostics,
        non_diagnostic_output,
    })
}

/// Execute a test build.
///
/// Test builds may report diagnostics for generated drivers using their
/// source-tree paths. The test build metadata lets the diagnostic processing
/// stage resolve those paths through the target layout.
pub(crate) fn execute_test_build(
    cfg: &BuildConfig,
    input: BuildInput,
    target_dir: &Path,
    build_metas: &[&BuildMeta],
    user_log: &UserLog,
) -> anyhow::Result<BuildStats> {
    let execution = execute_build_capturing(cfg, input, target_dir)?;
    let sources = execution.diagnostic_sources(build_metas.iter().copied());
    let processed = process_captured_diagnostics(&sources, cfg);
    processed.warn_if_limited(user_log);
    Ok(BuildStats {
        n_tasks_executed: execution.n_tasks_executed,
        n_errors: processed.n_errors,
        n_warnings: processed.n_warnings,
    })
}

/// Rebuild only the requested outputs and their prerequisites, for example
/// after snapshot promotion. The caller holds the same target lock as a full build.
#[instrument(skip_all)]
pub(crate) fn execute_build_partial<'a>(
    cfg: &BuildConfig,
    input: BuildInput,
    target_dir: &Path,
    build_meta: Option<&BuildMeta>,
    user_log: &UserLog,
    outputs: impl IntoIterator<Item = &'a Path>,
) -> anyhow::Result<BuildStats> {
    let execution = n2::execute(cfg, &input, target_dir, outputs)?;
    Ok(finish_captured_build(cfg, &execution, build_meta, user_log))
}

fn execute_build_capturing(
    cfg: &BuildConfig,
    input: BuildInput,
    target_dir: &Path,
) -> anyhow::Result<CapturedBuildExecution> {
    n2::execute(
        cfg,
        &input,
        target_dir,
        input.execution_plan.default_output_paths(),
    )
}

fn finish_captured_build(
    cfg: &BuildConfig,
    execution: &CapturedBuildExecution,
    build_meta: Option<&BuildMeta>,
    user_log: &UserLog,
) -> BuildStats {
    let sources = execution.diagnostic_sources(build_meta);
    let processed = process_captured_diagnostics(&sources, cfg);
    processed.warn_if_limited(user_log);
    BuildStats {
        n_tasks_executed: execution.n_tasks_executed,
        n_errors: processed.n_errors,
        n_warnings: processed.n_warnings,
    }
}

fn should_render_non_diagnostic_build_output(cfg: &BuildConfig, build_succeeded: bool) -> bool {
    !(cfg.suppress_progress && build_succeeded)
}

struct CapturedDiagnosticSource<'a> {
    diagnostics: &'a ResultCatcher,
    build_succeeded: bool,
    build_meta: Option<&'a BuildMeta>,
}

struct ProcessedDiagnostics {
    n_errors: usize,
    n_warnings: usize,
    hidden_errors: usize,
    hidden_warnings: usize,
}

struct CollectedJsonDiagnostics {
    processed: ProcessedDiagnostics,
    diagnostics: Vec<String>,
    non_diagnostic_output: Vec<String>,
}

impl ProcessedDiagnostics {
    fn warn_if_limited(&self, user_log: &UserLog) {
        if self.hidden_errors != 0 || self.hidden_warnings != 0 {
            user_log.warn(format!(
                "diagnostic output limited by --diagnostic-limit: {} errors and {} warnings were not displayed.",
                self.hidden_errors, self.hidden_warnings
            ));
        }
    }
}

fn rewrite_captured_diagnostic(
    content: &str,
    cfg: &BuildConfig,
    build_meta: Option<&BuildMeta>,
) -> String {
    let Some(meta) = build_meta else {
        return content.to_owned();
    };
    let layout = meta.artifact_paths.target_layout();
    let packages = &meta.resolve_output.pkg_dirs;
    let backend = meta.target_backend();

    if cfg.output_style.needs_moonc_json() {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(content) else {
            return content.to_owned();
        };
        let mut changed = false;
        let mut diagnostics = vec![&mut value];
        while let Some(diagnostic) = diagnostics.pop() {
            let Some(object) = diagnostic.as_object_mut() else {
                continue;
            };
            if let Some(serde_json::Value::String(path)) = object.get_mut("path")
                && let Some(physical) =
                    layout.generated_test_driver_diagnostic_path(packages, Path::new(path), backend)
            {
                *path = physical.to_string_lossy().into_owned();
                changed = true;
            }
            if let Some(serde_json::Value::Array(children)) = object.get_mut("children") {
                diagnostics.extend(children.iter_mut());
            }
        }
        return if changed {
            serde_json::to_string(&value).expect("diagnostic JSON should serialize")
        } else {
            content.to_owned()
        };
    }

    let Some(prefix_start) = content.find(GENERATED_TEST_DRIVER_PREFIX) else {
        return content.to_owned();
    };
    let Some(extension_end) = content[prefix_start..].find(".mbt") else {
        return content.to_owned();
    };
    let path_end = prefix_start + extension_end + ".mbt".len();
    let Some(physical) = layout.generated_test_driver_diagnostic_path(
        packages,
        Path::new(&content[..path_end]),
        backend,
    ) else {
        return content.to_owned();
    };
    format!("{}{}", physical.display(), &content[path_end..])
}

fn collect_json_diagnostics(
    sources: &[CapturedDiagnosticSource<'_>],
    cfg: &BuildConfig,
    retain_suppressed_output: bool,
) -> CollectedJsonDiagnostics {
    let mut catcher = ResultCatcher::default();
    for source in sources {
        catcher.n_errors += source.diagnostics.n_errors;
        catcher.n_warnings += source.diagnostics.n_warnings;
    }

    let mut by_file = BTreeMap::<String, BTreeSet<(MooncDiagnostic, String)>>::new();
    let mut non_diagnostic_output = Vec::new();
    for source in sources {
        for content in &source.diagnostics.content_writer {
            let content = rewrite_captured_diagnostic(content, cfg, source.build_meta);
            match serde_json::from_str::<MooncDiagnostic>(&content) {
                Ok(diagnostic) => {
                    if diagnostic_is_generated_test_driver_warning(&diagnostic) {
                        continue;
                    }
                    by_file
                        .entry(diagnostic.path.clone())
                        .or_default()
                        .insert((diagnostic, content));
                }
                Err(_) => {
                    if retain_suppressed_output
                        || should_render_non_diagnostic_build_output(cfg, source.build_succeeded)
                    {
                        non_diagnostic_output.push(content);
                    }
                }
            }
        }
    }

    let mut diagnostics = Vec::new();
    let (hidden_errors, hidden_warnings) = match cfg.diagnostic_limit {
        None => {
            for file_diagnostics in by_file.values() {
                for (diagnostic, content) in file_diagnostics {
                    diagnostics.push(content.clone());
                    catcher.append_diag(diagnostic);
                }
            }
            (0, 0)
        }
        Some(limit) => {
            let mut displayed = 0;
            let mut hidden_errors = 0;
            let mut total_warnings = 0;
            let mut displayed_warnings = 0;
            let mut non_errors = Vec::new();

            for file_diagnostics in by_file.values() {
                for (diagnostic, content) in file_diagnostics {
                    if diagnostic_is_error(diagnostic) {
                        if displayed < limit {
                            diagnostics.push(content.clone());
                            catcher.append_diag(diagnostic);
                            displayed += 1;
                        } else {
                            hidden_errors += 1;
                        }
                        continue;
                    }

                    if diagnostic_is_warning(diagnostic) {
                        total_warnings += 1;
                    }
                    if displayed < limit {
                        non_errors.push((diagnostic, content));
                    }
                }
            }

            if displayed < limit {
                for (diagnostic, content) in non_errors {
                    diagnostics.push(content.clone());
                    catcher.append_diag(diagnostic);
                    displayed += 1;
                    if diagnostic_is_warning(diagnostic) {
                        displayed_warnings += 1;
                    }
                    if displayed == limit {
                        break;
                    }
                }
            }

            let hidden_warnings = total_warnings - displayed_warnings;
            catcher.n_errors += hidden_errors;
            catcher.n_warnings += hidden_warnings;
            (hidden_errors, hidden_warnings)
        }
    };

    CollectedJsonDiagnostics {
        processed: ProcessedDiagnostics {
            n_errors: catcher.n_errors,
            n_warnings: catcher.n_warnings,
            hidden_errors,
            hidden_warnings,
        },
        diagnostics,
        non_diagnostic_output,
    }
}

fn process_captured_diagnostics(
    sources: &[CapturedDiagnosticSource<'_>],
    cfg: &BuildConfig,
) -> ProcessedDiagnostics {
    if cfg.output_style == OutputStyle::Json {
        let collected = collect_json_diagnostics(sources, cfg, false);
        for content in &collected.non_diagnostic_output {
            eprintln!("{content}");
        }
        for content in &collected.diagnostics {
            println!("{content}");
        }
        return collected.processed;
    }

    let mut catcher = ResultCatcher::default();
    for source in sources {
        catcher.n_errors += source.diagnostics.n_errors;
        catcher.n_warnings += source.diagnostics.n_warnings;
    }
    let mut hidden_errors_total = 0;
    let mut hidden_warnings_total = 0;
    let captured = sources.iter().flat_map(|source| {
        source
            .diagnostics
            .content_writer
            .iter()
            .map(move |content| {
                (
                    rewrite_captured_diagnostic(content, cfg, source.build_meta),
                    source.build_succeeded,
                )
            })
    });

    match cfg.output_style {
        OutputStyle::Json => unreachable!(),
        OutputStyle::Fancy => {
            let mut by_file = BTreeMap::<String, BTreeSet<MooncDiagnostic>>::new();
            for (content, build_succeeded) in captured {
                match serde_json::from_str::<moonutil::render::MooncDiagnostic>(&content) {
                    Ok(d) => {
                        if diagnostic_is_generated_test_driver_warning(&d) {
                            continue;
                        }
                        by_file.entry(d.path.clone()).or_default().insert(d);
                    }
                    Err(_) => {
                        // Non-diagnostics output, just print as-is
                        // This could happen for installing binaries dependencies etc.
                        if should_render_non_diagnostic_build_output(cfg, build_succeeded) {
                            eprintln!("{content}");
                        }
                    }
                };
            }

            let patch_file = cfg.patch_file.as_ref();
            match cfg.diagnostic_limit {
                None => {
                    for file_diagnostics in by_file.values() {
                        for diag in file_diagnostics {
                            let kind = diag.render_diagnostics(
                                n2::use_fancy(),
                                patch_file,
                                cfg.explain_errors,
                                cfg.render_no_loc,
                            );
                            catcher.append_kind(kind);
                        }
                    }
                }
                Some(limit) => {
                    let build_config = cfg;
                    let mut displayed = 0;
                    let mut hidden_errors = 0;
                    let mut total_warnings = 0;
                    let mut displayed_warnings = 0;
                    let mut non_errors = Vec::new();

                    for file_diagnostics in by_file.values() {
                        for diag in file_diagnostics {
                            if !diagnostic_is_renderable(diag, build_config) {
                                continue;
                            }

                            if diagnostic_is_error(diag) {
                                if displayed < limit {
                                    let kind = diag.render_diagnostics(
                                        n2::use_fancy(),
                                        patch_file,
                                        build_config.explain_errors,
                                        build_config.render_no_loc,
                                    );
                                    catcher.append_kind(kind);
                                    displayed += 1;
                                } else {
                                    hidden_errors += 1;
                                }
                                continue;
                            }

                            if diagnostic_is_warning(diag) {
                                total_warnings += 1;
                            }
                            if displayed < limit {
                                non_errors.push(diag);
                            }
                        }
                    }

                    if displayed < limit {
                        for diag in non_errors {
                            let kind = diag.render_diagnostics(
                                n2::use_fancy(),
                                patch_file,
                                build_config.explain_errors,
                                build_config.render_no_loc,
                            );
                            catcher.append_kind(kind);
                            displayed += 1;
                            if diagnostic_is_warning(diag) {
                                displayed_warnings += 1;
                            }
                            if displayed == limit {
                                break;
                            }
                        }
                    }

                    let hidden_warnings = total_warnings - displayed_warnings;
                    hidden_errors_total += hidden_errors;
                    hidden_warnings_total += hidden_warnings;
                    catcher.n_errors += hidden_errors;
                    catcher.n_warnings += hidden_warnings;
                }
            }
        }
        OutputStyle::Raw => {
            for (content, _) in captured {
                println!("{content}");
            }
        }
    }
    ProcessedDiagnostics {
        n_errors: catcher.n_errors,
        n_warnings: catcher.n_warnings,
        hidden_errors: hidden_errors_total,
        hidden_warnings: hidden_warnings_total,
    }
}

fn diagnostic_is_error(diag: &MooncDiagnostic) -> bool {
    diag.level == "error"
}

fn diagnostic_is_warning(diag: &MooncDiagnostic) -> bool {
    matches!(diag.level.as_str(), "warn" | "warning")
}

fn diagnostic_is_generated_test_driver_warning(diag: &MooncDiagnostic) -> bool {
    diagnostic_is_warning(diag) && diag.path.contains("__generated_driver_for_")
}

fn diagnostic_is_renderable(diag: &MooncDiagnostic, cfg: &BuildConfig) -> bool {
    if !diag.path.is_empty() {
        return true;
    }

    DiagnosticLevel::from_str(&diag.level, true).is_ok_and(|level| level >= cfg.render_no_loc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonutil::render::{Loc, Position};

    fn diagnostic(path: &str, level: &str) -> MooncDiagnostic {
        MooncDiagnostic {
            path: path.to_string(),
            loc: Loc {
                start: Position { line: 1, col: 1 },
                end: Position { line: 1, col: 2 },
            },
            level: level.to_string(),
            message: String::new(),
            error_code: 0,
            children: Vec::new(),
        }
    }

    #[test]
    fn suppresses_generated_test_driver_warnings_only() {
        assert!(diagnostic_is_generated_test_driver_warning(&diagnostic(
            "./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt",
            "warning"
        )));
        assert!(!diagnostic_is_generated_test_driver_warning(&diagnostic(
            "./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt",
            "error"
        )));
        assert!(!diagnostic_is_generated_test_driver_warning(&diagnostic(
            "./lib/hello.mbt",
            "warning"
        )));
    }

    #[test]
    fn generated_test_driver_errors_are_counted() {
        let generated_driver_path =
            "./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt";
        let warning = diagnostic(generated_driver_path, "warning");
        let error = diagnostic(generated_driver_path, "error");
        let mut catcher = ResultCatcher::default();
        catcher.append_content(serde_json::to_string(&warning).unwrap());
        catcher.append_content(serde_json::to_string(&error).unwrap());

        let cfg = BuildConfig {
            output_style: OutputStyle::Json,
            ..Default::default()
        };
        let processed = process_captured_diagnostics(
            &[CapturedDiagnosticSource {
                diagnostics: &catcher,
                build_succeeded: false,
                build_meta: None,
            }],
            &cfg,
        );

        assert_eq!(processed.n_warnings, 0);
        assert_eq!(processed.n_errors, 1);
    }

    #[test]
    fn diagnostic_limit_is_shared_across_captured_build_errors() {
        let mut dependency_error = diagnostic("./dependency.mbt", "error");
        dependency_error
            .children
            .push(diagnostic("./dependency-detail.mbt", "error"));
        let script_error = diagnostic("./script.mbt", "error");
        let mut dependency = ResultCatcher::default();
        dependency.append_content(serde_json::to_string(&dependency_error).unwrap());
        let mut script = ResultCatcher::default();
        script.append_content(serde_json::to_string(&script_error).unwrap());

        let cfg = BuildConfig {
            output_style: OutputStyle::Json,
            diagnostic_limit: Some(1),
            ..Default::default()
        };
        let processed = process_captured_diagnostics(
            &[
                CapturedDiagnosticSource {
                    diagnostics: &dependency,
                    build_succeeded: true,
                    build_meta: None,
                },
                CapturedDiagnosticSource {
                    diagnostics: &script,
                    build_succeeded: false,
                    build_meta: None,
                },
            ],
            &cfg,
        );

        assert_eq!(processed.n_errors, 2);
        assert_eq!(processed.n_warnings, 0);
        assert_eq!(processed.hidden_errors, 1);
        assert_eq!(processed.hidden_warnings, 0);
    }

    #[test]
    fn structured_json_retains_successful_output_when_progress_is_suppressed() {
        let mut catcher = ResultCatcher::default();
        catcher.append_content("PREBUILD_SUCCESS");
        let cfg = BuildConfig::default().with_suppressed_progress(true);
        let sources = [CapturedDiagnosticSource {
            diagnostics: &catcher,
            build_succeeded: true,
            build_meta: None,
        }];

        let collected = collect_json_diagnostics(&sources, &cfg, true);
        assert_eq!(collected.non_diagnostic_output, ["PREBUILD_SUCCESS"]);

        let rendered = collect_json_diagnostics(&sources, &cfg, false);
        assert!(rendered.non_diagnostic_output.is_empty());
    }
}
