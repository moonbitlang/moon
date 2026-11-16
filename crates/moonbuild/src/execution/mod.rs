// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Execute a concrete plan and process captured diagnostics.
//!
//! The private `n2` module owns graph adaptation, scheduling, and the database.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    sync::LazyLock,
};

use anyhow::Context;
use ariadne::ReportKind;
use clap::ValueEnum;
use colored::Colorize;
use moonbuild_rupes_recta::{
    execution_plan::{ActionId, ActionOutputKind, ExecutionPlan, LoweredCommandExecution},
    target_layout::GENERATED_TEST_DRIVER_PREFIX,
};
use moonutil::{
    render::{DiagnosticSources, MooncDiagnostic},
    target::TargetBackend,
    test_metadata::DiagnosticLevel,
    user_log::UserLog,
};
use tracing::instrument;

use crate::BuildMeta;

pub mod action_identity;
mod n2;

/// Execution and diagnostic options. Planning inputs live in `BuildInput`.
#[derive(Clone)]
pub struct BuildConfig {
    /// The level of parallelism to use. If `None`, will use the number of
    /// available CPU cores.
    pub parallelism: Option<usize>,
    /// The output style for errors and warnings
    pub output_style: OutputStyle,
    /// Render no-location diagnostics above this level
    pub render_no_loc: DiagnosticLevel,
    /// Maximum number of diagnostics to display after deduplication.
    pub diagnostic_limit: Option<usize>,

    /// Explain and warnings in diagnostics
    pub explain_errors: bool,

    /// Ask n2 to explain rerun reasons
    pub n2_explain: bool,

    /// Verbose output for build progress and command echo
    pub verbose: bool,
    pub suppress_progress: bool,

    /// The patch file to use
    pub patch_file: Option<PathBuf>,
}

impl BuildConfig {
    pub fn with_suppressed_progress(mut self, suppress_progress: bool) -> Self {
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

/// The style to render diagnostics in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputStyle {
    /// The human-readable raw format directly from `moonc`
    Raw,
    /// Source code snippets with colors and formatting, rendered from JSON
    Fancy,
    /// Machine-readable output in JSON
    Json,
}

impl OutputStyle {
    /// Whether the output style requires `moonc` to emit JSON diagnostics.
    pub fn needs_moonc_json(&self) -> bool {
        matches!(self, OutputStyle::Fancy | OutputStyle::Json)
    }
}

/// Share the default observation between prebuild scripts, build execution, and
/// test execution so they see the same job limit throughout this Moon process.
pub fn resolve_parallelism(jobs: Option<usize>) -> usize {
    static DEFAULT_PARALLELISM: LazyLock<usize> =
        LazyLock::new(|| std::thread::available_parallelism().map_or(1, usize::from));
    jobs.unwrap_or_else(|| *DEFAULT_PARALLELISM)
}

/// A complete execution plan and the context needed to execute it.
#[derive(Debug, Clone)]
pub struct BuildInput {
    /// Executor-neutral actions shared by execution and its projections.
    execution_plan: Rc<ExecutionPlan>,

    /// Target Backend for each action. Shared actions have no single backend.
    action_backends: HashMap<ActionId, Option<TargetBackend>>,
}

impl BuildInput {
    pub fn new(execution_plan: ExecutionPlan, backend: Option<TargetBackend>) -> Self {
        let action_backends = execution_plan
            .action_ids()
            .map(|id| (id, backend))
            .collect();
        Self {
            execution_plan: Rc::new(execution_plan),
            action_backends,
        }
    }

    pub fn execution_plan(&self) -> &ExecutionPlan {
        &self.execution_plan
    }

    /// The Target Backend of an action, or `None` for an action shared by
    /// several backends.
    pub fn action_backend(&self, id: ActionId) -> Option<TargetBackend> {
        self.action_backends.get(&id).copied().flatten()
    }

    pub fn compose(inputs: Vec<Self>) -> anyhow::Result<Self> {
        let mut inputs = inputs.into_iter();
        let first = inputs
            .next()
            .context("cannot compose an empty build invocation")?;
        let Some(second) = inputs.next() else {
            return Ok(first);
        };

        let mut execution_plan = ExecutionPlan::default();
        let mut action_backends = HashMap::new();

        for input in std::iter::once(first)
            .chain(std::iter::once(second))
            .chain(inputs)
        {
            let existing_actions = execution_plan.action_ids().collect::<HashSet<_>>();
            let remapped = execution_plan.merge(&input.execution_plan)?;
            for (old, new) in input.execution_plan.action_ids().zip(remapped) {
                if existing_actions.contains(&new) {
                    if action_backends.get(&new) != input.action_backends.get(&old) {
                        action_backends.insert(new, None);
                    }
                } else {
                    action_backends.insert(new, input.action_backends.get(&old).copied().flatten());
                }
            }
        }

        Ok(Self {
            execution_plan: Rc::new(execution_plan),
            action_backends,
        })
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
pub struct BuildStats {
    /// Number of build tasks executed, `None` means build failure
    n_tasks_executed: Option<usize>,

    n_errors: usize,
    n_warnings: usize,
}

impl BuildStats {
    /// Whether the run was successful (i.e. didn't fail to execute).
    pub fn successful(&self) -> bool {
        self.n_tasks_executed.is_some()
    }

    /// Get the return code that should be returned to the shell.
    pub fn return_code_for_success(&self) -> i32 {
        if self.successful() { 0 } else { 1 }
    }

    pub fn print_info(&self, quiet: bool, mode: &str) -> anyhow::Result<()> {
        self.print_summary(quiet);
        if !self.successful() {
            anyhow::bail!("failed when {mode} project");
        }
        Ok(())
    }

    /// Print the build summary without converting a failed build into an error.
    /// Callers that own their exit status can use `successful` directly.
    pub fn print_summary(&self, quiet: bool) {
        match self.n_tasks_executed {
            None => {
                // These are compiler diagnostic counts, not failed-task counts.
                // Raw output is not parsed, and tool failures may have no
                // compiler diagnostics. Do not describe either as "0 errors".
                if self.n_warnings > 0 || self.n_errors > 0 {
                    eprintln!(
                        "Failed with {} warnings, {} errors.",
                        self.n_warnings, self.n_errors
                    );
                }
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
    }
}

struct CapturedBuildExecution<'a> {
    /// Action IDs are only meaningful within this input's execution plan.
    input: &'a BuildInput,
    n_tasks_executed: Option<usize>,
    action_outputs: Vec<CapturedActionOutput>,
}

struct CapturedActionOutput {
    action_id: ActionId,
    content: ResultCatcher,
    /// Executor status, independent of whether an additional message is needed.
    failure: Option<ActionFailureKind>,
}

/// Failure presentation is derived after capture; UserLog controls visibility.
enum FailureMessage<'a> {
    Error(String),
    Command(&'a str),
}

/// The unsuccessful termination reported by the executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionFailureKind {
    /// A nonzero exit, abnormal termination, or per-task execution error. This
    /// does not establish that the compiler completed normally, even when it
    /// emitted error diagnostics.
    // TODO: Preserve exit codes and signals when n2 exposes them separately.
    Failed,
    Interrupted,
}

/// One failed execution action, retained independently of diagnostic rendering.
/// Ordinary compiler errors also produce a failed action. A diagnostic that
/// explains a failure does not turn it into a successful action.
#[derive(Debug, PartialEq, Eq)]
pub struct ActionFailure {
    pub kind: ActionFailureKind,
    pub target_backend: Option<TargetBackend>,
    pub output_kind: ActionOutputKind,
    /// A parsed compiler error was captured before any diagnostic filtering.
    /// This does not prove the compiler completed normally after emitting it.
    pub has_error_diagnostics: bool,
}

impl CapturedBuildExecution<'_> {
    fn successful(&self) -> bool {
        self.n_tasks_executed.is_some()
    }

    fn failures(
        &self,
        cfg: &BuildConfig,
    ) -> impl Iterator<Item = (ActionFailure, FailureMessage<'_>)> {
        self.action_outputs.iter().filter_map(|output| {
            let kind = output.failure?;
            let action = self.input.execution_plan.action(output.action_id);
            // Classify the original output before diagnostic filtering. Raw
            // output is opaque; only compiler actions can emit Moonc diagnostics.
            let has_error_diagnostics = cfg.output_style.needs_moonc_json()
                && action.output_kind() == ActionOutputKind::Compiler
                && output.content.content_writer.iter().any(|line| {
                    serde_json::from_str::<MooncDiagnostic>(line)
                        .is_ok_and(|diagnostic| diagnostic.level == "error")
                });
            let has_failure_output = if cfg.output_style.needs_moonc_json() {
                has_error_diagnostics
            } else {
                output
                    .content
                    .content_writer
                    .iter()
                    .any(|line| !line.trim().is_empty())
            };
            let command = match action.command().execution() {
                LoweredCommandExecution::Inline(command)
                | LoweredCommandExecution::ResponseFile { command, .. } => command,
            };
            let description = action.description();
            let message = match kind {
                ActionFailureKind::Failed if has_failure_output => FailureMessage::Command(command),
                ActionFailureKind::Failed => {
                    FailureMessage::Error(format!("failed to {description}: {command}"))
                }
                ActionFailureKind::Interrupted => FailureMessage::Error(format!(
                    "Interrupted while attempting to {description}: {command}"
                )),
            };
            Some((
                ActionFailure {
                    kind,
                    target_backend: self.input.action_backend(output.action_id),
                    output_kind: action.output_kind(),
                    has_error_diagnostics,
                },
                message,
            ))
        })
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
                let build_meta = self
                    .input
                    .action_backend(output.action_id)
                    .and_then(|backend| {
                        build_metas
                            .iter()
                            .find(|meta| meta.target_backend() == backend)
                            .copied()
                    })
                    .or(sole_build_meta);
                CapturedDiagnosticSource {
                    diagnostics: &output.content,
                    output_kind: self
                        .input
                        .execution_plan
                        .action(output.action_id)
                        .output_kind(),
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
pub fn execute_build(
    cfg: &BuildConfig,
    input: BuildInput,
    target_dir: &Path,
    user_log: &UserLog,
) -> anyhow::Result<BuildStats> {
    let execution = execute_build_capturing(cfg, &input, target_dir)?;
    Ok(finish_captured_build(cfg, &execution, None, user_log))
}

/// Structured output from one build execution for a command-level JSON
/// renderer. The executor does not write diagnostics or summaries itself.
pub struct JsonBuildOutput {
    pub n_tasks_executed: Option<usize>,
    pub n_errors: usize,
    pub n_warnings: usize,
    pub hidden_errors: usize,
    pub hidden_warnings: usize,
    pub diagnostics: Vec<JsonBuildDiagnostic>,
    /// All unsuccessful actions, including compiler errors that do not need an
    /// additional Moon-authored error message. Independent of diagnostic limits
    /// and verbosity. An empty list does not establish build success; use
    /// `successful()`. Graph setup and scheduling errors are returned through
    /// `Result`.
    pub action_failures: Vec<ActionFailure>,
    /// Moon-authored task failures, separate from compiler diagnostics.
    pub action_errors: Vec<String>,
    /// Commands for failures already explained by tool output. Log at info
    /// level so UserLog determines their visibility.
    pub failed_commands: Vec<String>,
    pub non_diagnostic_output: Vec<String>,
}

/// One compiler diagnostic and the backend of the action that emitted it.
/// The command layer remains responsible for projecting this into its JSON
/// schema.
pub struct JsonBuildDiagnostic {
    pub target_backend: Option<TargetBackend>,
    pub value: serde_json::Value,
}

impl JsonBuildOutput {
    pub fn successful(&self) -> bool {
        self.n_tasks_executed.is_some()
    }
}

/// Execute a build while returning all Moonc diagnostics to the CLI seam.
pub fn execute_build_json(
    cfg: &BuildConfig,
    input: BuildInput,
    target_dir: &Path,
) -> anyhow::Result<JsonBuildOutput> {
    let execution = execute_build_capturing(cfg, &input, target_dir)?;
    Ok(collect_captured_build_json(cfg, execution))
}

fn collect_captured_build_json(
    cfg: &BuildConfig,
    execution: CapturedBuildExecution<'_>,
) -> JsonBuildOutput {
    // Keep the existing per-backend diagnostic-limit semantics while all
    // backends execute in one n2 graph. Shared actions have no backend and are
    // collected in their own group.
    let mut sources_by_backend = BTreeMap::new();
    for output in &execution.action_outputs {
        sources_by_backend
            .entry(execution.input.action_backend(output.action_id))
            .or_insert_with(Vec::new)
            .push(CapturedDiagnosticSource {
                diagnostics: &output.content,
                output_kind: execution
                    .input
                    .execution_plan
                    .action(output.action_id)
                    .output_kind(),
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

    let mut action_failures = Vec::new();
    let mut action_errors = Vec::new();
    let mut failed_commands = Vec::new();
    for (failure, message) in execution.failures(cfg) {
        action_failures.push(failure);
        match message {
            FailureMessage::Error(error) => action_errors.push(error),
            FailureMessage::Command(command) => failed_commands.push(command.to_owned()),
        }
    }

    JsonBuildOutput {
        n_tasks_executed: execution.n_tasks_executed,
        n_errors,
        n_warnings,
        hidden_errors,
        hidden_warnings,
        diagnostics,
        action_failures,
        failed_commands,
        action_errors,
        non_diagnostic_output,
    }
}

/// Execute a test build.
///
/// Test builds may report diagnostics for generated drivers using their
/// source-tree paths. The test build metadata lets the diagnostic processing
/// stage resolve those paths through the target layout.
pub fn execute_test_build(
    cfg: &BuildConfig,
    input: BuildInput,
    target_dir: &Path,
    build_metas: &[&BuildMeta],
    user_log: &UserLog,
) -> anyhow::Result<BuildStats> {
    let execution = execute_build_capturing(cfg, &input, target_dir)?;
    Ok(finish_captured_build(
        cfg,
        &execution,
        build_metas.iter().copied(),
        user_log,
    ))
}

/// Rebuild only the requested outputs and their prerequisites, for example
/// after snapshot promotion. The caller holds the same target lock as a full build.
#[instrument(skip_all)]
pub fn execute_build_partial<'a>(
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

fn execute_build_capturing<'a>(
    cfg: &BuildConfig,
    input: &'a BuildInput,
    target_dir: &Path,
) -> anyhow::Result<CapturedBuildExecution<'a>> {
    n2::execute(
        cfg,
        input,
        target_dir,
        input.execution_plan.default_output_paths(),
    )
}

fn finish_captured_build<'a>(
    cfg: &BuildConfig,
    execution: &'a CapturedBuildExecution<'_>,
    build_metas: impl IntoIterator<Item = &'a BuildMeta>,
    user_log: &UserLog,
) -> BuildStats {
    for (_, message) in execution.failures(cfg) {
        match message {
            FailureMessage::Error(error) => user_log.error(error),
            FailureMessage::Command(command) => user_log.info(format!("Failed command: {command}")),
        }
    }
    let sources = execution.diagnostic_sources(build_metas);
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
    output_kind: ActionOutputKind,
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
    let packages = &meta.resolved_project.pkg_dirs;
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
            let content = if source.output_kind == ActionOutputKind::Compiler {
                rewrite_captured_diagnostic(content, cfg, source.build_meta)
            } else {
                content.clone()
            };
            let diagnostic = match source.output_kind {
                ActionOutputKind::Compiler => {
                    serde_json::from_str::<MooncDiagnostic>(&content).ok()
                }
                ActionOutputKind::Tool => None,
            };
            match diagnostic {
                Some(diagnostic) => {
                    if diagnostic_is_generated_test_driver_warning(&diagnostic) {
                        continue;
                    }
                    by_file
                        .entry(diagnostic.path.clone())
                        .or_default()
                        .insert((diagnostic, content));
                }
                None => {
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
                    if source.output_kind == ActionOutputKind::Compiler {
                        rewrite_captured_diagnostic(content, cfg, source.build_meta)
                    } else {
                        content.clone()
                    },
                    source.build_succeeded,
                    source.output_kind,
                )
            })
    });

    match cfg.output_style {
        OutputStyle::Json => unreachable!(),
        OutputStyle::Fancy => {
            let mut by_file = BTreeMap::<String, BTreeSet<MooncDiagnostic>>::new();
            for (content, build_succeeded, output_kind) in captured {
                let diagnostic = match output_kind {
                    ActionOutputKind::Compiler => {
                        serde_json::from_str::<MooncDiagnostic>(&content).ok()
                    }
                    ActionOutputKind::Tool => None,
                };
                match diagnostic {
                    Some(d) => {
                        if diagnostic_is_generated_test_driver_warning(&d) {
                            continue;
                        }
                        by_file.entry(d.path.clone()).or_default().insert(d);
                    }
                    None => {
                        // Non-diagnostics output, just print as-is
                        // This could happen for installing binaries dependencies etc.
                        if should_render_non_diagnostic_build_output(cfg, build_succeeded) {
                            eprintln!("{content}");
                        }
                    }
                };
            }

            let patch_file = cfg.patch_file.as_ref();
            let mut sources = DiagnosticSources::default();
            match cfg.diagnostic_limit {
                None => {
                    for file_diagnostics in by_file.values() {
                        for diag in file_diagnostics {
                            let kind = diag.render_diagnostics(
                                n2::use_fancy(),
                                patch_file,
                                &mut sources,
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
                                        &mut sources,
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
                                &mut sources,
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
            for (content, _, _) in captured {
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

    fn composed_build_input() -> BuildInput {
        use moonbuild_rupes_recta::{
            CompileConfig, ProjectPreparationConfig,
            build_lower::WarningCondition,
            build_plan::{ArtifactKey, InputDirective},
            fmt::{FmtConfig, build_execution_plan_for_fmt_file},
            model::{BackendConfig, TargetKind},
            target_layout::{ArtifactPathResolver, TargetLayout, TargetLayoutMode},
        };
        use moonutil::{
            build_options::RunMode,
            cond_expr::OptLevel,
            project::WorkspaceEnv,
            resolution::{DEFAULT_VERSION, DirSyncResult, ModuleDependencyGraph, ModuleSource},
            user_log::user_log_level,
        };

        // Use real plans so backend attribution is checked after composition
        // remaps each plan's action IDs, rather than duplicating metadata in
        // the captured-output fixture.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("moon.mod"), "name = \"test/capture\"\n").unwrap();
        std::fs::write(dir.path().join("moon.pkg"), "").unwrap();
        let source = dir.path().join("main.mbt");
        std::fs::write(&source, "fn main() {}\n").unwrap();
        let module = ModuleSource::local_path(
            "test/capture".parse().unwrap(),
            dir.path().to_owned(),
            DEFAULT_VERSION.clone(),
        )
        .unwrap();
        let (_, manifest) = moonutil::manifest::read_module_manifest(dir.path(), false).unwrap();
        let (modules, module_id) = ModuleDependencyGraph::only_one_module(module, manifest);
        let mut dirs = DirSyncResult::default();
        dirs.insert(module_id, dir.path().to_owned());
        let user_log = UserLog::new(user_log_level(false, true));
        let resolved = moonbuild_rupes_recta::prepare_synced_project(
            &ProjectPreparationConfig::new_with_load_defaults(false, false, WorkspaceEnv::Auto),
            (modules, dirs),
            &[TargetBackend::WasmGC, TargetBackend::Js],
            &user_log,
        )
        .unwrap();
        let package = resolved
            .pkg_dirs
            .get_package_id_by_name("test/capture")
            .unwrap();
        let target_dir = dir.path().join("_build");
        let mut inputs = [BackendConfig::WasmGc { use_wat: false }, BackendConfig::Js]
            .into_iter()
            .map(|backend| {
                let target = backend.target_backend();
                let cfg = CompileConfig {
                    target_dir: target_dir.clone(),
                    backend,
                    opt_level: OptLevel::Debug,
                    action: RunMode::Check,
                    debug_info: Default::default(),
                    stdlib_path: None,
                    artifact_paths: ArtifactPathResolver::new(
                        TargetLayout::new(
                            target_dir.clone(),
                            TargetLayoutMode::Workspace,
                            OptLevel::Debug,
                            RunMode::Check,
                        ),
                        None,
                    ),
                    debug_export_build_plan: false,
                    enable_coverage: false,
                    moonc_output_json: true,
                    docs_serve: false,
                    warning_condition: WarningCondition::Default,
                    warn_list: None,
                };
                let compiled = moonbuild_rupes_recta::compile(
                    &cfg,
                    &resolved,
                    &[ArtifactKey::CheckMi {
                        package,
                        target_kind: TargetKind::Source,
                    }],
                    &InputDirective::default(),
                    None,
                    &user_log,
                )
                .unwrap();
                BuildInput::new(compiled.execution_plan, Some(target))
            })
            .collect::<Vec<_>>();
        let tool = build_execution_plan_for_fmt_file(
            &FmtConfig {
                check_only: true,
                extra_args: vec![],
                warn_only: false,
            },
            &source,
            &target_dir,
        )
        .unwrap();
        inputs.insert(1, BuildInput::new(tool, None));
        BuildInput::compose(inputs).unwrap()
    }

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
    fn action_failures_are_independent_of_diagnostic_limits_and_messages() {
        // The compiler failure needs no extra error message. The tool emits
        // compiler-shaped JSON, and the interrupted action emits no output.
        // None of these presentation differences may erase the task result.
        let input = composed_build_input();
        let action_ids = input.execution_plan.action_ids().collect::<Vec<_>>();
        assert_eq!(action_ids.len(), 3);
        for diagnostic_limit in [None, Some(0)] {
            let cfg = BuildConfig {
                output_style: OutputStyle::Json,
                diagnostic_limit,
                ..Default::default()
            };
            let compiler_error = serde_json::to_string(&diagnostic("main.mbt", "error")).unwrap();
            // Concurrent actions can finish in a different order from the plan.
            let action_outputs = [
                (
                    action_ids[1],
                    ActionFailureKind::Failed,
                    Some(compiler_error.clone()),
                ),
                (
                    action_ids[0],
                    ActionFailureKind::Failed,
                    Some(compiler_error.clone()),
                ),
                (action_ids[2], ActionFailureKind::Interrupted, None),
            ]
            .map(|(action_id, failure, output)| CapturedActionOutput {
                action_id,
                failure: Some(failure),
                content: ResultCatcher {
                    content_writer: output.into_iter().collect(),
                    ..Default::default()
                },
            });
            let result = collect_captured_build_json(
                &cfg,
                CapturedBuildExecution {
                    input: &input,
                    n_tasks_executed: None,
                    action_outputs: action_outputs.into(),
                },
            );
            assert!(!result.successful());
            assert_eq!(result.n_errors, 1);
            assert_eq!(
                result.hidden_errors,
                usize::from(diagnostic_limit == Some(0))
            );
            assert_eq!(
                result.diagnostics.len(),
                usize::from(diagnostic_limit.is_none())
            );
            assert_eq!(result.action_errors.len(), 2);
            assert!(result.action_errors[0].starts_with("failed to format "));
            assert!(result.action_errors[1].starts_with("Interrupted while attempting to check "));
            assert_eq!(result.failed_commands.len(), 1);
            assert!(result.failed_commands[0].contains("-target wasm-gc"));
            assert_eq!(result.non_diagnostic_output, [compiler_error]);
            assert_eq!(
                result.action_failures,
                [
                    ActionFailure {
                        kind: ActionFailureKind::Failed,
                        target_backend: None,
                        output_kind: ActionOutputKind::Tool,
                        has_error_diagnostics: false,
                    },
                    ActionFailure {
                        kind: ActionFailureKind::Failed,
                        target_backend: Some(TargetBackend::WasmGC),
                        output_kind: ActionOutputKind::Compiler,
                        has_error_diagnostics: true,
                    },
                    ActionFailure {
                        kind: ActionFailureKind::Interrupted,
                        target_backend: Some(TargetBackend::Js),
                        output_kind: ActionOutputKind::Compiler,
                        has_error_diagnostics: false,
                    },
                ]
            );
        }
    }

    #[test]
    fn diagnostic_output_does_not_create_an_action_failure() {
        let input = composed_build_input();
        let cfg = BuildConfig {
            output_style: OutputStyle::Json,
            ..Default::default()
        };
        let result = collect_captured_build_json(
            &cfg,
            CapturedBuildExecution {
                input: &input,
                n_tasks_executed: Some(1),
                action_outputs: vec![CapturedActionOutput {
                    action_id: input.execution_plan.action_ids().next().unwrap(),
                    content: ResultCatcher {
                        content_writer: vec![
                            serde_json::to_string(&diagnostic("main.mbt", "error")).unwrap(),
                        ],
                        ..Default::default()
                    },
                    failure: None,
                }],
            },
        );
        assert!(result.successful());
        assert_eq!(result.n_errors, 1);
        assert!(result.action_failures.is_empty());
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
                output_kind: ActionOutputKind::Compiler,
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
                    output_kind: ActionOutputKind::Compiler,
                    build_succeeded: true,
                    build_meta: None,
                },
                CapturedDiagnosticSource {
                    diagnostics: &script,
                    output_kind: ActionOutputKind::Compiler,
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
    fn tool_output_is_not_parsed_as_compiler_diagnostics() {
        // A tool can print the same JSON as the compiler. Only the compiler
        // source participates in counting, filtering, and diagnostic limits.
        for level in ["error", "warning"] {
            let message = serde_json::to_string(&diagnostic("./script.mbt", level)).unwrap();
            let mut output = ResultCatcher::default();
            output.append_content(&message);
            let cfg = BuildConfig {
                output_style: OutputStyle::Json,
                diagnostic_limit: Some(0),
                ..Default::default()
            };
            let sources = [ActionOutputKind::Compiler, ActionOutputKind::Tool].map(|output_kind| {
                CapturedDiagnosticSource {
                    diagnostics: &output,
                    output_kind,
                    build_succeeded: false,
                    build_meta: None,
                }
            });
            let collected = collect_json_diagnostics(&sources, &cfg, true);
            assert!(collected.diagnostics.is_empty());
            assert_eq!(collected.non_diagnostic_output, [message]);
            assert_eq!(collected.processed.n_errors, usize::from(level == "error"));
            assert_eq!(
                collected.processed.n_warnings,
                usize::from(level == "warning")
            );
            assert_eq!(
                collected.processed.hidden_errors,
                usize::from(level == "error")
            );
            assert_eq!(
                collected.processed.hidden_warnings,
                usize::from(level == "warning")
            );
        }
    }

    #[test]
    fn structured_json_retains_successful_output_when_progress_is_suppressed() {
        let mut catcher = ResultCatcher::default();
        catcher.append_content("PREBUILD_SUCCESS");
        let cfg = BuildConfig::default().with_suppressed_progress(true);
        let sources = [CapturedDiagnosticSource {
            diagnostics: &catcher,
            output_kind: ActionOutputKind::Tool,
            build_succeeded: true,
            build_meta: None,
        }];

        let collected = collect_json_diagnostics(&sources, &cfg, true);
        assert_eq!(collected.non_diagnostic_output, ["PREBUILD_SUCCESS"]);

        let rendered = collect_json_diagnostics(&sources, &cfg, false);
        assert!(rendered.non_diagnostic_output.is_empty());
    }
}
