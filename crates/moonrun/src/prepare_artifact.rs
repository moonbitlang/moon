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

//! Moon preparation owns its helper invocation and the delivered files.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Context;
use moonutil::run_handoff::{HANDOFF_ARG, PreparedProgram, RunHandoff};

use crate::RunOutcome;
use crate::run_termination::TerminationRequest;
use crate::runtime::RuntimeConfig;
use crate::runtime::Stdio as RuntimeStdio;

pub(crate) enum Preparation {
    Ready {
        program: Box<PreparedProgram>,
        directory: tempfile::TempDir,
    },
    Exited(RunOutcome),
}

pub(crate) fn prepare_moon(
    executable: &Path,
    args: &[OsString],
    config: &RuntimeConfig,
    request: &TerminationRequest,
    #[cfg(windows)] no_console_window: bool,
) -> anyhow::Result<Preparation> {
    if let Some(outcome) = request.get() {
        return Ok(Preparation::Exited(outcome.into()));
    }
    let directory = tempfile::tempdir().context("failed to create Moon artifact handoff")?;
    let handoff = RunHandoff::new(
        directory.path(),
        config.inherited_policy.is_some() || config.policy_file.is_some(),
    )?;
    let mut command = Command::new(executable);
    command
        .arg(HANDOFF_ARG)
        .arg(handoff.directory())
        .args(args)
        .current_dir(config.working_directory.current_dir()?);
    if let Some(environment) = &config.default_environment {
        command.env_clear().envs(environment.iter().cloned());
    }
    command.env_remove(moonutil::constants::MOONRUN_INHERITED_POLICY);
    if let RuntimeStdio::Owned(files) = &config.stdio {
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
    if let Some(mask) = config.preserved_child_signal_mask {
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
        return Ok(Preparation::Exited(outcome.into()));
    }
    let status = status?;
    if !status.success() {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(signal) = status.signal() {
                return Ok(Preparation::Exited(RunOutcome::KilledBySignal(signal)));
            }
        }
        return Ok(Preparation::Exited(RunOutcome::Exited(
            status.code().unwrap_or(1),
        )));
    }
    Ok(Preparation::Ready {
        program: Box::new(handoff.load()?),
        directory,
    })
}
