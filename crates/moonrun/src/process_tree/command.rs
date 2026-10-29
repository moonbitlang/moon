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

//! Execute one supervised command, including preparation and artifact cleanup.
//!
//! The supervisor owns admission and the termination receiver. This module
//! borrows control across every stage; Engine only executes a Wasm Run.

mod moon;

use std::path::Path;

use crate::run_termination::TerminationRequest;
use crate::{Engine, RunOptions, RunOutcome, WorkingDirectory};

pub(super) fn run_moon(
    engine: &Engine,
    executable: &Path,
    args: &[std::ffi::OsString],
    mut options: RunOptions,
    signals: crate::SignalReceiver,
    termination: &TerminationRequest,
    #[cfg(windows)] no_console_window: bool,
) -> anyhow::Result<RunOutcome> {
    let (program, _directory) = match moon::prepare(
        executable,
        args,
        &options.runtime,
        termination,
        #[cfg(windows)]
        no_console_window,
    )? {
        moon::Preparation::Ready { program, directory } => (*program, directory),
        moon::Preparation::Exited(outcome) => return Ok(outcome),
    };
    options.args = program.args;
    options.runtime.working_directory = WorkingDirectory::Fixed(program.working_directory);
    if options.runtime.inherited_policy.is_none()
        && options.runtime.policy_file.is_none()
        && let Some((policy, source_dir)) = program.policy
    {
        options = options.with_policy_file_source_dir(policy, source_dir);
    }
    // The delivery directory outlives Runtime construction and execution. Its
    // cleanup finishes before the supervisor finalizes the command result.
    run_executable(
        engine,
        program.execution_kind,
        &program.artifact,
        &program.executable,
        |_| {
            program
                .source_map
                .as_deref()
                .and_then(crate::source_map::SourceMap::parse)
        },
        options,
        signals,
        termination,
    )
}

// Root executables and Moon delivery results share dispatch. Keep artifact
// location and original identity explicit; they differ after a handoff.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_executable(
    engine: &Engine,
    kind: moonutil::run_handoff::ExecutionKind,
    artifact: &Path,
    origin: &Path,
    source_map: impl FnOnce(&[u8]) -> Option<crate::source_map::SourceMap>,
    options: RunOptions,
    signals: crate::SignalReceiver,
    termination: &TerminationRequest,
) -> anyhow::Result<RunOutcome> {
    match kind {
        moonutil::run_handoff::ExecutionKind::Wasm => {
            let bytes = std::fs::read(artifact)?;
            let module = engine.compile_file(origin, &bytes, || source_map(&bytes))?;
            engine.run_in_scope(&module, options, signals, termination)
        }
        moonutil::run_handoff::ExecutionKind::Native | moonutil::run_handoff::ExecutionKind::Js => {
            // FIXME: Admit native executables and Node through the tree's native
            // process nodes, sharing PID, wait, cancellation and reaping ownership.
            anyhow::bail!(
                "native process nodes (including Node) are not yet supported in virtual process trees"
            )
        }
    }
}
