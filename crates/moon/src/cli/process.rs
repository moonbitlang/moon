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

use std::{
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

/// The only process-wide actions a selected Moon invocation may request.
pub(crate) enum ProcessAction {
    Exit(i32),
    Delegate(Command),
    DelegateWithPolicyRelay(Command, moonutil::policy_transport::PolicyRelay),
}

impl From<i32> for ProcessAction {
    fn from(code: i32) -> Self {
        Self::Exit(code)
    }
}

/// Execute an action after the invocation's tracing state has been dropped.
pub(crate) fn execute(action: ProcessAction) -> anyhow::Result<i32> {
    match action {
        ProcessAction::Exit(code) => Ok(code),
        ProcessAction::Delegate(mut command) => Ok(delegate(&mut command)?.code().unwrap_or(0)),
        ProcessAction::DelegateWithPolicyRelay(mut command, relay) => {
            Ok(delegate_with_policy_relay(&mut command, relay)?
                .code()
                .unwrap_or(0))
        }
    }
}

#[cfg(unix)]
fn delegate_with_policy_relay(
    cmd: &mut Command,
    relay: moonutil::policy_transport::PolicyRelay,
) -> anyhow::Result<ExitStatus> {
    let _relay = relay.attach_to(cmd)?;
    delegate(cmd)
}

#[cfg(windows)]
fn delegate_with_policy_relay(
    cmd: &mut Command,
    relay: moonutil::policy_transport::PolicyRelay,
) -> anyhow::Result<ExitStatus> {
    install_ctrl_c_passthrough_handler()?;
    // Moonx reaches this point after registry acquisition or standalone build
    // work and does not start other children. Keep Windows' process-wide
    // inheritable-handle window to the CreateProcess call itself.
    let relay = relay.attach_to(cmd)?;
    let child = cmd.spawn();
    let isolation = relay.finish();
    match (child, isolation) {
        (Err(error), _) => Err(error.into()),
        (Ok(mut child), Ok(())) => Ok(child.wait()?),
        (Ok(mut child), Err(error)) => {
            // The parent must not continue with the policy handle globally
            // inheritable. Do not leave an already-created delegate running
            // after reporting that isolation failed.
            let _ = child.kill();
            let _ = child.wait();
            Err(error.into())
        }
    }
}

/// Construct a delegated command whose executable lookup and child working
/// directory use the same effective command directory.
pub(crate) fn command_in_effective_dir(
    current_dir: Option<&Path>,
    resolve_program: impl FnOnce(Option<&Path>) -> anyhow::Result<PathBuf>,
) -> anyhow::Result<Command> {
    let mut command = Command::new(resolve_program(current_dir)?);
    if let Some(dir) = current_dir {
        command.current_dir(dir);
    }
    Ok(command)
}

/// Delegate to a command as directly as the platform allows.
///
/// On Unix this replaces the current process. On Windows there is no direct
/// equivalent, so we run the child and wait while letting it handle Ctrl-C.
/// Use this only for command paths that return directly to process exit.
#[cfg(unix)]
fn delegate(cmd: &mut Command) -> anyhow::Result<ExitStatus> {
    use std::os::unix::prelude::*;

    Err(cmd.exec().into())
}

/// Keep the parent alive while its Windows console delivers Ctrl-C to children.
#[cfg(windows)]
pub(crate) fn install_ctrl_c_passthrough_handler() -> anyhow::Result<()> {
    use anyhow::bail;
    use windows_sys::Win32::Foundation::{BOOL, FALSE, TRUE};
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

    unsafe extern "system" fn ctrlc_handler(_: u32) -> BOOL {
        // The child receives the console event independently.
        TRUE
    }

    unsafe {
        if SetConsoleCtrlHandler(Some(ctrlc_handler), TRUE) == FALSE {
            bail!("could not set Ctrl-C handler")
        }
    }
    Ok(())
}

/// Delegate to a command as directly as the platform allows.
///
/// On Unix this replaces the current process. On Windows there is no direct
/// equivalent, so we run the child and wait while letting it handle Ctrl-C.
/// Use this only for command paths that return directly to process exit.
#[cfg(windows)]
fn delegate(cmd: &mut Command) -> anyhow::Result<ExitStatus> {
    install_ctrl_c_passthrough_handler()?;
    Ok(cmd.status()?)
}
