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

//! n2 graph adaptation, incremental state, scheduling, and output capture.
//!
//! The database belongs to this executor; artifact paths belong to
//! `moonbuild_rupes_recta::target_layout`. Callers hold the target-directory lock
//! across execution and any other access to mutable build artifacts.

use std::{collections::HashMap, path::Path, rc::Rc, sync::mpsc};

use anyhow::Context;
use moonbuild_rupes_recta::execution_plan::{ActionOutputKind, LoweredCommandExecution};

use super::resolve_parallelism;
use super::{BuildConfig, BuildInput, CapturedActionOutput, CapturedBuildExecution, ResultCatcher};

// Progress and compiler diagnostics share terminal setup, including enabling
// virtual terminal processing on Windows.
// TODO: Move this into shared CLI terminal handling when it owns setup for both
// progress and diagnostics.
pub(super) use ::n2::terminal::use_fancy;

pub(super) fn execute<'a>(
    cfg: &BuildConfig,
    input: &BuildInput,
    target_dir: &Path,
    outputs: impl IntoIterator<Item = &'a Path>,
) -> anyhow::Result<CapturedBuildExecution> {
    use n2::graph::{Build, BuildIns, BuildOuts, FileLoc, Graph, RspFile};

    let mut graph = Graph::default();
    let mut context_by_build = HashMap::with_capacity(input.action_backends.len());
    for id in input.execution_plan.action_ids() {
        let action = input.execution_plan.action(id);
        let inputs = action
            .inputs()
            .iter()
            .map(|path| {
                graph
                    .files
                    .id_from_canonical(path.to_string_lossy().into_owned())
            })
            .collect::<Vec<_>>();
        let outputs = action
            .outputs()
            .iter()
            .map(|path| {
                graph
                    .files
                    .id_from_canonical(path.to_string_lossy().into_owned())
            })
            .collect::<Vec<_>>();
        let mut build = Build::new(
            FileLoc {
                filename: Rc::new(action.fileloc().into()),
                line: 0,
            },
            BuildIns {
                explicit: inputs.len(),
                ids: inputs,
                implicit: 0,
                order_only: 0,
            },
            BuildOuts {
                explicit: outputs.len(),
                ids: outputs,
            },
        );
        let (command, rspfile) = match action.command().execution() {
            LoweredCommandExecution::Inline(command) => (command, None),
            LoweredCommandExecution::ResponseFile { command, file } => (
                command,
                Some(RspFile {
                    path: file.path.clone(),
                    content: file.content.clone(),
                }),
            ),
        };
        build.cmdline = Some(command.clone());
        build.rspfile = rspfile;
        build.cwd = action.command().cwd().map(|cwd| cwd.display().to_string());
        build.env = action.command().env().to_vec();
        build.desc = Some(action.description().to_owned());
        build.can_dirty_on_output = action.can_dirty_on_output();
        let build_id = graph.add_build(build).with_context(|| {
            format!(
                "Failed to adapt package {}, action {id:?} to n2",
                action.error_package()
            )
        })?;
        context_by_build.insert(
            build_id,
            (
                input.action_backends.get(&id).copied().flatten(),
                action.description().to_owned(),
                command.clone(),
                action.output_kind(),
            ),
        );
    }

    std::fs::create_dir_all(target_dir).with_context(|| {
        format!(
            "Failed to create target directory: '{}'",
            target_dir.display()
        )
    })?;
    // All backends, profiles, and run modes share this database. Their concrete
    // output paths distinguish their state. The caller's target lock covers
    // database access as well as preparation and consumption of mutable outputs.
    let db_path = target_dir.join(".moon_db");
    let mut hashes = n2::graph::Hashes::default();
    let db = n2::db::open(&db_path, &mut graph, &mut hashes)
        .with_context(|| format!("Failed to open build cache DB at {}", db_path.display()))?;

    let (sender, receiver) = mpsc::channel();
    let output_style = cfg.output_style;
    let verbose = cfg.verbose;
    let callback: Box<n2::progress::BuildResultCallback> =
        Box::new(move |build_id, termination, output: &str| {
            if termination == n2::progress::Termination::Success && output.is_empty() {
                return;
            }
            let (target_backend, description, command, output_kind) = context_by_build
                .get(&build_id)
                .expect("every n2 build should retain its action context");
            let mut captured = ResultCatcher::default();
            for line in output.split('\n').filter(|line| !line.is_empty()) {
                captured.append_content(line);
            }
            // Raw output is opaque: preserve the tool's explanation without
            // trying to classify it as JSON or adding a duplicate error.
            // Structured compiler errors explain failures even when hidden by
            // the diagnostic limit. Silent failures always need action context.
            let has_failure_output = if output_style.needs_moonc_json() {
                *output_kind == ActionOutputKind::Compiler
                    && captured.content_writer.iter().any(|line| {
                        serde_json::from_str::<moonutil::render::MooncDiagnostic>(line)
                            .is_ok_and(|diagnostic| diagnostic.level == "error")
                    })
            } else {
                !output.trim().is_empty()
            };
            let error = match termination {
                n2::progress::Termination::Failure if !has_failure_output => {
                    Some(format!("failed to {description}: {command}"))
                }
                n2::progress::Termination::Interrupted => Some(format!(
                    "Interrupted while attempting to {description}: {command}"
                )),
                _ => None,
            };
            sender
                .send(CapturedActionOutput {
                    target_backend: *target_backend,
                    output_kind: *output_kind,
                    content: captured,
                    error,
                    failed_command: (verbose
                        && termination == n2::progress::Termination::Failure
                        && has_failure_output)
                        .then(|| command.clone()),
                })
                .expect("captured output receiver should outlive n2 progress");
        });
    let mut progress: Box<dyn n2::progress::Progress> = if !cfg.suppress_progress && use_fancy() {
        Box::new(n2::progress::FancyConsoleProgress::new(
            cfg.verbose,
            Some(callback),
        ))
    } else {
        Box::new(n2::progress::DumbConsoleProgress::new(
            cfg.verbose,
            Some(callback),
        ))
    };
    // Use exactly the spellings registered above. Work::lookup applies n2's
    // lexical normalization, which does not preserve Windows verbatim prefixes.
    let requested_outputs = outputs
        .into_iter()
        .map(|path| {
            graph
                .files
                .lookup(&path.to_string_lossy())
                .with_context(|| format!("Unknown requested build output: {}", path.display()))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut work = n2::work::Work::new(
        graph,
        hashes,
        db,
        &n2::work::Options {
            failures_left: Some(10), // FIXME: This value matches legacy; make it configurable when needed.
            parallelism: resolve_parallelism(cfg.parallelism),
            explain: cfg.n2_explain,
            adopt: false,
            dirty_on_output: true,
        },
        &mut *progress,
        n2::smallmap::SmallMap::default(),
    );
    for file in requested_outputs {
        work.want_file(file)
            .context("Failed to determine the files to be built")?;
    }
    let result = work.run().context("Failed to run n2 graph");
    drop(work);
    drop(progress); // Finish the progress display before rendering diagnostics.
    Ok(CapturedBuildExecution {
        n_tasks_executed: result?,
        action_outputs: receiver.into_iter().collect(),
    })
}
