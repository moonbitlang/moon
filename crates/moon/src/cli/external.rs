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

use anyhow::{Context as _, bail};
use std::process::{Command, ExitStatus};
use which::which_global;

pub fn run_external(mut args: Vec<String>) -> anyhow::Result<i32> {
    if args.is_empty() {
        bail!("no external subcommand provided");
    };
    let subcmd = args.remove(0);
    let bin = format!("moon-{subcmd}");
    let resolved = which_global(&bin).context(anyhow::format_err!(
        "no such subcommand: `{subcmd}`, is `{bin}` a valid executable accessible via your `PATH`?"
    ))?;
    Ok(exec(Command::new(resolved).args(args))?.code().unwrap_or(0))
}

#[cfg(unix)]
fn exec(cmd: &mut Command) -> anyhow::Result<ExitStatus> {
    use std::os::unix::prelude::*;

    Err(cmd.exec().into())
}

#[cfg(windows)]
fn exec(cmd: &mut Command) -> anyhow::Result<ExitStatus> {
    use windows_sys::Win32::Foundation::{BOOL, FALSE, TRUE};
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

    unsafe extern "system" fn ctrlc_handler(_: u32) -> BOOL {
        // Do nothing. Let the child process handle it.
        TRUE
    }

    unsafe {
        if SetConsoleCtrlHandler(Some(ctrlc_handler), TRUE) == FALSE {
            bail!("could not set Ctrl-C handler")
        }
    }

    Ok(cmd.status()?)
}
