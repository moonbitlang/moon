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

use std::process::{Command, ExitStatus};

/// Delegate to a command as directly as the platform allows.
///
/// On Unix this replaces the current process. On Windows there is no direct
/// equivalent, so we run the child and wait while letting it handle Ctrl-C.
/// Use this only for command paths that return directly to process exit.
#[cfg(unix)]
pub(crate) fn delegate(cmd: &mut Command) -> anyhow::Result<ExitStatus> {
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
pub(crate) fn delegate(cmd: &mut Command) -> anyhow::Result<ExitStatus> {
    install_ctrl_c_passthrough_handler()?;
    Ok(cmd.status()?)
}
