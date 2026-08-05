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
use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use super::process;

pub(crate) fn run_external(mut args: Vec<String>) -> anyhow::Result<i32> {
    if args.is_empty() {
        bail!("no external subcommand provided");
    };
    let subcmd = args.remove(0);
    let mut command = process::command_in_effective_dir(None, |current_dir| {
        resolve_external_subcommand_in(&subcmd, current_dir)
    })?;
    Ok(process::delegate(command.args(args))?.code().unwrap_or(0))
}

pub(crate) fn run_external_help(
    subcmd: &str,
    current_dir: Option<&Path>,
    args: impl IntoIterator<Item = OsString>,
) -> anyhow::Result<i32> {
    let mut cmd = process::command_in_effective_dir(current_dir, |current_dir| {
        resolve_external_subcommand_in(subcmd, current_dir)
    })?;
    run_external_command(&mut cmd, args)
        .with_context(|| format!("Unable to get help from `{subcmd}` utility"))?
        .code()
        .ok_or_else(|| anyhow::anyhow!("Unable to get exit code"))
}

fn resolve_external_subcommand_in(
    subcmd: &str,
    current_dir: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    if subcmd == "-" {
        bail!(
            "`-` is only supported in `moon run -`, which reads `.mbtx` source from stdin.\n\
             Try: `moon run -`"
        );
    }
    let bin = &format!("moon-{subcmd}");
    let resolved = match current_dir {
        Some(dir) => moonutil::toolchain::resolve_executable_in(bin, dir),
        None => moonutil::toolchain::resolve_executable(bin),
    };
    resolved.with_context(|| {
        format!(
            "no such subcommand: `{subcmd}`, is `{bin}` a valid executable accessible via your `PATH`?"
        )
    })
}

fn run_external_command(
    cmd: &mut Command,
    args: impl IntoIterator<Item = OsString>,
) -> anyhow::Result<ExitStatus> {
    process::delegate(cmd.args(args))
}

pub(crate) fn run_ide_help_if_requested(raw_args: &[OsString]) -> Option<anyhow::Result<i32>> {
    let (current_dir, args) = ide_help_args(raw_args)?;
    Some(run_external_help("ide", current_dir.as_deref(), args))
}

fn ide_help_args(raw_args: &[OsString]) -> Option<(Option<PathBuf>, Vec<OsString>)> {
    let early = process::early_subcommand(raw_args)?;
    if early.name != OsStr::new("help") {
        return None;
    }
    let [ide, tail @ ..] = early.args else {
        return None;
    };
    if ide != OsStr::new("ide") {
        return None;
    }

    let mut delegated = tail.to_vec();
    delegated.push(OsString::from("--help"));
    Some((early.current_dir, delegated))
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
            ide_help_args(&os(&[
                "moon",
                "--target-dir=_build-alt",
                "-qvC=.",
                "--trace",
                "help",
                "ide",
                "doc"
            ])),
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
