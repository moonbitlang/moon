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

//! Build helpers return an artifact; only this thread executes its Wasm.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Context;
use moonutil::run_handoff::{HANDOFF_ARG, RunHandoff};

use crate::runtime::Stdio as RuntimeStdio;
use crate::{Engine, RunOptions, RunOutcome, WorkingDirectory};

pub(super) fn run(
    engine: &Engine,
    executable: &Path,
    args: &[OsString],
    mut options: RunOptions,
    signals: crate::SignalReceiver,
    #[cfg(windows)] no_console_window: bool,
) -> anyhow::Result<RunOutcome> {
    let request = options
        .termination_receiver
        .as_ref()
        .expect("the tree owns termination")
        .request
        .clone();
    let result = (|| {
        if let Some(outcome) = request.get() {
            return Ok(outcome.into());
        }
        let directory = tempfile::tempdir().context("failed to create Moon artifact handoff")?;
        let handoff = RunHandoff::new(
            directory.path(),
            options.inherited_policy.is_some() || options.policy_file.is_some(),
        )?;
        let mut command = Command::new(executable);
        command
            .arg(HANDOFF_ARG)
            .arg(handoff.directory())
            .args(args)
            .current_dir(options.working_directory.current_dir()?)
            .env_clear()
            .envs(
                options
                    .default_environment
                    .as_ref()
                    .expect("the tree owns the environment")
                    .iter()
                    .cloned(),
            )
            .env_remove(moonutil::constants::MOONRUN_INHERITED_POLICY);
        if let RuntimeStdio::Owned(files) = &options.stdio {
            command
                .stdin(Stdio::from(files[0].lock().unwrap().try_clone()?))
                .stdout(Stdio::from(files[1].lock().unwrap().try_clone()?))
                .stderr(Stdio::from(files[2].lock().unwrap().try_clone()?));
        }
        #[cfg(windows)]
        if no_console_window {
            use std::os::windows::process::CommandExt;
            command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
        }
        #[cfg(unix)]
        if let Some(mask) = options.preserved_child_signal_mask {
            use std::os::unix::process::CommandExt;
            // Restore only the explicit pre-broker mask, using an async-signal-safe
            // syscall in the forked child. No Rust locks or allocation here.
            unsafe {
                command.pre_exec(move || {
                    if libc::sigprocmask(libc::SIG_SETMASK, &mask, std::ptr::null_mut()) == -1 {
                        Err(std::io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                });
            }
        }
        // FIXME: Add cancellation of the helper and its compiler descendants as a
        // unit (Unix process group / Windows Job Object). Until then, retain a kill
        // request and reap the helper normally before publishing completion.
        let status = command.status().context("failed to run Moon build helper");
        drop(command);
        if let Some(outcome) = request.get() {
            return Ok(outcome.into());
        }
        let status = status?;
        if !status.success() {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(signal) = status.signal() {
                    return Ok(RunOutcome::KilledBySignal(signal));
                }
            }
            return Ok(RunOutcome::Exited(status.code().unwrap_or(1)));
        }
        let prepared = handoff.load()?;
        options.args = prepared.args;
        options.working_directory = WorkingDirectory::Fixed(prepared.working_directory);
        if options.inherited_policy.is_none()
            && options.policy_file.is_none()
            && let Some((policy, source_dir)) = prepared.policy
        {
            options = options.with_policy_file_source_dir(policy, source_dir);
        }
        let module = engine.compile_file(&prepared.executable, &prepared.wasm, || {
            prepared
                .source_map
                .as_deref()
                .and_then(crate::source_map::SourceMap::parse)
        })?;
        // Keep the handoff directory, including copied policy sources, alive until
        // Runtime construction and execution have finished.
        engine.run_with_signal_receiver(&module, options, signals)
    })();
    match request.close() {
        Some(outcome) => Ok(outcome.into()),
        None => result,
    }
}
