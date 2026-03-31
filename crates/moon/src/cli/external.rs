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
use clap::error::ErrorKind;
use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
    process::{Command, ExitStatus},
};
use which::which_global;

pub(crate) fn run_external(mut args: Vec<String>) -> anyhow::Result<i32> {
    if args.is_empty() {
        bail!("no external subcommand provided");
    };
    let subcmd = args.remove(0);
    let resolved = resolve_external_subcommand(&subcmd)?;
    Ok(exec(Command::new(resolved).args(args))?.code().unwrap_or(0))
}

pub(crate) fn run_external_help(
    subcmd: &str,
    args: impl IntoIterator<Item = OsString>,
) -> anyhow::Result<i32> {
    run_external_command(resolve_external_subcommand(subcmd)?, args)
        .with_context(|| format!("Unable to get help from `{subcmd}` utility"))?
        .code()
        .ok_or_else(|| anyhow::anyhow!("Unable to get exit code"))
}

fn resolve_external_subcommand(subcmd: &str) -> anyhow::Result<PathBuf> {
    if subcmd == "-" {
        bail!(
            "`-` is only supported in `moon run -`, which reads `.mbtx` source from stdin.\n\
             Try: `moon run -`"
        );
    }
    let bin = &format!("moon-{subcmd}");
    which_global(bin).context(anyhow::format_err!(
        "no such subcommand: `{subcmd}`, is `{bin}` a valid executable accessible via your `PATH`?"
    ))
}

fn run_external_command(
    program: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = OsString>,
) -> std::io::Result<ExitStatus> {
    Command::new(program).args(args).status()
}

pub(crate) fn exit_if_ide_help_request(err: &clap::Error, raw_args: &[OsString]) {
    if err.kind() != ErrorKind::InvalidSubcommand {
        return;
    }

    let Some(args) = ide_help_args(raw_args) else {
        return;
    };
    match run_external_help("ide", args) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("Error: {err:?}");
            std::process::exit(-1);
        }
    }
}

fn ide_help_args(raw_args: &[OsString]) -> Option<Vec<OsString>> {
    let [_, help, ide, tail @ ..] = raw_args else {
        return None;
    };
    if help != OsStr::new("help") || ide != OsStr::new("ide") {
        return None;
    }

    let mut delegated = tail.to_vec();
    delegated.push(OsString::from("--help"));
    Some(delegated)
}

#[cfg(test)]
mod tests {
    use super::ide_help_args;
    use std::ffi::OsString;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn delegates_top_level_help_for_ide() {
        assert_eq!(
            ide_help_args(&os(&["moon", "help", "ide"])),
            Some(os(&["--help"]))
        );
    }

    #[test]
    fn delegates_subcommand_help_for_ide() {
        assert_eq!(
            ide_help_args(&os(&["moon", "help", "ide", "doc"])),
            Some(os(&["doc", "--help"]))
        );
    }

    #[test]
    fn ignores_other_help_targets() {
        assert_eq!(ide_help_args(&os(&["moon", "help", "build"])), None);
    }

    #[test]
    fn ignores_regular_ide_execution() {
        assert_eq!(ide_help_args(&os(&["moon", "ide", "--help"])), None);
    }
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
