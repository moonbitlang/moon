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
    path::{Path, PathBuf},
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
    current_dir: Option<&Path>,
    args: impl IntoIterator<Item = OsString>,
) -> anyhow::Result<i32> {
    let mut cmd = Command::new(resolve_external_subcommand(subcmd)?);
    if let Some(dir) = current_dir {
        cmd.current_dir(dir);
    }
    run_external_command(&mut cmd, args)
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
    cmd: &mut Command,
    args: impl IntoIterator<Item = OsString>,
) -> std::io::Result<ExitStatus> {
    cmd.args(args).status()
}

pub(crate) fn exit_if_ide_help_request(err: &clap::Error, raw_args: &[OsString]) {
    if err.kind() != ErrorKind::InvalidSubcommand {
        return;
    }

    let Some((current_dir, args)) = ide_help_args(raw_args) else {
        return;
    };
    match run_external_help("ide", current_dir.as_deref(), args) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("Error: {err:?}");
            std::process::exit(-1);
        }
    }
}

fn ide_help_args(raw_args: &[OsString]) -> Option<(Option<PathBuf>, Vec<OsString>)> {
    match raw_args {
        [_, help, ide, tail @ ..] if help == OsStr::new("help") && ide == OsStr::new("ide") => {
            let mut delegated = tail.to_vec();
            delegated.push(OsString::from("--help"));
            Some((None, delegated))
        }
        [_, chdir, dir, help, ide, tail @ ..]
            if chdir == OsStr::new("-C")
                && help == OsStr::new("help")
                && ide == OsStr::new("ide") =>
        {
            let mut delegated = tail.to_vec();
            delegated.push(OsString::from("--help"));
            Some((Some(PathBuf::from(dir)), delegated))
        }
        _ => None,
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

#[cfg(test)]
mod tests {
    use super::ide_help_args;
    use std::{ffi::OsString, path::PathBuf};

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn delegates_top_level_help_for_ide() {
        assert_eq!(
            ide_help_args(&os(&["moon", "help", "ide"])),
            Some((None, os(&["--help"])))
        );
    }

    #[test]
    fn delegates_subcommand_help_for_ide() {
        assert_eq!(
            ide_help_args(&os(&["moon", "help", "ide", "doc"])),
            Some((None, os(&["doc", "--help"])))
        );
    }

    #[test]
    fn delegates_help_for_ide_after_chdir() {
        assert_eq!(
            ide_help_args(&os(&["moon", "-C", ".", "help", "ide", "doc"])),
            Some((Some(PathBuf::from(".")), os(&["doc", "--help"])))
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
