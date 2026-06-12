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

//! Hidden command runner for build-system generated commands.

use std::{ffi::OsString, path::PathBuf, process::Command};

use anyhow::{Context, bail};
use clap::Parser;

#[derive(Debug, clap::Parser)]
pub(crate) struct Exec {
    /// Change to this directory before executing the command.
    #[clap(long)]
    cwd: Option<PathBuf>,

    /// Add or override an environment variable, in KEY=VALUE form.
    #[clap(long = "env", value_name = "KEY=VALUE")]
    envs: Vec<String>,

    /// Remove an environment variable.
    #[clap(long = "unset-env", value_name = "KEY")]
    unset_envs: Vec<String>,

    /// Run a platform shell command string.
    #[clap(long, value_name = "COMMAND")]
    shell: Option<String>,

    /// Run a direct argv command. This is mutually exclusive with --shell.
    #[clap(last = true, allow_hyphen_values = true, value_name = "ARGV")]
    argv: Vec<OsString>,
}

#[derive(Debug, Eq, PartialEq)]
struct EnvAssignment {
    key: String,
    value: String,
}

impl EnvAssignment {
    fn parse(raw: &str) -> anyhow::Result<Self> {
        let Some((key, value)) = raw.split_once('=') else {
            bail!("environment assignment `{raw}` must use KEY=VALUE form");
        };
        if key.is_empty() {
            bail!("environment assignment `{raw}` has an empty key");
        }
        Ok(Self {
            key: key.to_string(),
            value: value.to_string(),
        })
    }
}

pub(crate) fn run_exec(cmd: Exec) -> anyhow::Result<i32> {
    let envs = cmd
        .envs
        .iter()
        .map(|raw| EnvAssignment::parse(raw))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut command = build_command(cmd.shell, cmd.argv)?;

    if let Some(cwd) = cmd.cwd {
        command.current_dir(cwd);
    }
    for env in envs {
        command.env(env.key, env.value);
    }
    for key in cmd.unset_envs {
        command.env_remove(key);
    }

    delegate(command)
}

pub(crate) fn is_tool_exec(raw_args: &[OsString]) -> bool {
    raw_args.get(1).is_some_and(|arg| arg == "tool")
        && raw_args.get(2).is_some_and(|arg| arg == "exec")
}

pub(crate) fn run_from_raw_args(raw_args: &[OsString]) -> anyhow::Result<i32> {
    let args =
        std::iter::once(OsString::from("moon tool exec")).chain(raw_args[3..].iter().cloned());
    run_exec(Exec::parse_from(args))
}

fn build_command(shell: Option<String>, argv: Vec<OsString>) -> anyhow::Result<Command> {
    match (shell, argv.is_empty()) {
        (Some(command), true) => build_shell_command(command),
        (Some(_), false) => bail!("--shell cannot be used together with direct argv"),
        (None, true) => bail!("either --shell or direct argv must be provided"),
        (None, false) => build_direct_command(argv),
    }
}

#[cfg(unix)]
fn build_shell_command(command: String) -> anyhow::Result<Command> {
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c").arg(command);
    Ok(cmd)
}

#[cfg(windows)]
fn build_shell_command(command: String) -> anyhow::Result<Command> {
    use std::os::windows::process::CommandExt;

    let (program, rest) = moonutil::shlex::split_argv0_windows(&command);
    if program.is_empty() {
        bail!("shell command is empty");
    }

    let mut cmd = Command::new(program);
    let rest = rest.trim_start();
    if !rest.is_empty() {
        cmd.raw_arg(rest);
    }
    Ok(cmd)
}

fn build_direct_command(argv: Vec<OsString>) -> anyhow::Result<Command> {
    let mut argv = argv.into_iter();
    let program = argv
        .next()
        .expect("direct argv is known to be non-empty before building command");
    let mut cmd = Command::new(program);
    cmd.args(argv);
    Ok(cmd)
}

#[cfg(unix)]
fn delegate(mut command: Command) -> anyhow::Result<i32> {
    use std::os::unix::process::CommandExt;

    let err = command.exec();
    Err(err).context("failed to exec command")
}

#[cfg(windows)]
fn delegate(mut command: Command) -> anyhow::Result<i32> {
    let status = command.status().context("failed to run command")?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_assignment_requires_equals() {
        let err = EnvAssignment::parse("MOON_TEST").expect_err("missing equals should fail");
        assert!(err.to_string().contains("KEY=VALUE"));
    }

    #[test]
    fn env_assignment_rejects_empty_key() {
        let err = EnvAssignment::parse("=value").expect_err("empty key should fail");
        assert!(err.to_string().contains("empty key"));
    }

    #[test]
    fn env_assignment_splits_on_first_equals() {
        assert_eq!(
            EnvAssignment::parse("MOON_TEST=a=b").expect("assignment should parse"),
            EnvAssignment {
                key: "MOON_TEST".to_string(),
                value: "a=b".to_string(),
            }
        );
    }
}
