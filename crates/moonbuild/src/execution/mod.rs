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
//! The private `diagnostics` module owns compiler-output interpretation and rendering.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    sync::LazyLock,
};

use anyhow::Context;
use colored::Colorize;
use moonbuild_rupes_recta::execution_plan::{
    ActionId, ActionOutputKind, ExecutionPlan, LoweredCommandExecution,
};
use moonutil::{target::TargetBackend, user_log::UserLog};
use tracing::instrument;

use crate::BuildMeta;

pub mod action_identity;
mod diagnostics;
mod n2;

pub use diagnostics::DiagnosticLevel;
use diagnostics::{
    CapturedDiagnosticSource, MooncDiagnostic, ResultCatcher, collect_json_diagnostics,
    process_captured_diagnostics,
};

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
