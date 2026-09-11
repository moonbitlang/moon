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

//! n2 graph adaptation, incremental state, scheduling, and output capture.
//!
//! The database belongs to this executor; artifact paths belong to
//! `moonbuild_rupes_recta::target_layout`. Callers hold the target-directory lock
//! across execution and any other access to mutable build artifacts.

use std::{collections::HashMap, path::Path, rc::Rc, sync::mpsc};

use anyhow::Context;
use moonbuild_rupes_recta::execution_plan::{InputObservation, LoweredCommandExecution};

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
    let mut backend_by_build = HashMap::with_capacity(input.action_backends.len());
    for id in input.execution_plan.action_ids() {
        let action = input.execution_plan.action(id);
        // Recursive interface observations participate in content identity.
        // n2 supports only concrete file observations.
        let inputs = action
            .inputs()
            .iter()
            .filter_map(|input| match input {
                InputObservation::File(path) => Some(path),
                InputObservation::StandardLibraryInterfaces(_) => None,
            })
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
        backend_by_build.insert(build_id, input.action_backends.get(&id).copied().flatten());
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
    let callback: Option<Box<n2::progress::BuildOutputCallback>> =
        Some(Box::new(move |build_id, output: &str| {
            let target_backend = backend_by_build
                .get(&build_id)
                .copied()
                .expect("every n2 build should retain its action backend");
            let mut captured = ResultCatcher::default();
            for line in output.split('\n').filter(|line| !line.is_empty()) {
                captured.append_content(line);
            }
            sender
                .send(CapturedActionOutput {
                    target_backend,
                    content: captured,
                })
                .expect("captured output receiver should outlive n2 progress");
        }));
    let mut progress: Box<dyn n2::progress::Progress> = if !cfg.suppress_progress && use_fancy() {
        Box::new(n2::progress::FancyConsoleProgress::new_with_build_output(
            cfg.verbose,
            callback,
        ))
    } else {
        Box::new(n2::progress::DumbConsoleProgress::new_with_build_output(
            cfg.verbose,
            callback,
        ))
    };
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
    for path in outputs {
        let file = work
            .lookup(&path.to_string_lossy())
            .with_context(|| format!("Unknown requested build output: {}", path.display()))?;
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
