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
    path::Path,
    process::{Command, Stdio},
};

use anyhow::bail;
use moonutil::cli_support::{
    MooncakeSubcommands, PackageSubcommand, PublishSubcommand, UniversalFlags,
};
use moonutil::user_log::UserLog;
use serde::Serialize;

use super::process;

pub(crate) fn execute_cli<T: Serialize>(
    cli: UniversalFlags,
    cmd: T,
    args: &[&str],
    display_name: &str,
) -> anyhow::Result<i32> {
    let current_moon = std::env::current_exe()?;
    let mut child = Command::new(&*moonutil::toolchain::BINARIES.mooncake)
        .args(args)
        .env("MOON_OVERRIDE", current_moon)
        .stdout(Stdio::inherit())
        .stdin(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    match child.stdin.take() {
        Some(mut stdin) => {
            let data = (cli, cmd);
            serde_json::ser::to_writer(&mut stdin, &data)?;
        }
        _ => {
            eprintln!("failed to open stdin");
        }
    }

    let status = child.wait()?;
    if status.success() {
        Ok(0)
    } else {
        bail!("`moon {}` failed", display_name)
    }
}

pub(crate) fn prepare_direct(current_dir: Option<&Path>, args: &[&str]) -> anyhow::Result<Command> {
    let current_moon = std::env::current_exe()?;
    let mut command = process::command_in_effective_dir(current_dir, |current_dir| {
        Ok(current_dir.map_or_else(
            || moonutil::toolchain::BINARIES.mooncake.clone(),
            moonutil::toolchain::mooncake_in,
        ))
    })?;
    command
        .args(args)
        .env("MOONCAKE_ALLOW_DIRECT", "1")
        .env("MOON_OVERRIDE", current_moon)
        .stdout(Stdio::inherit())
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit());
    Ok(command)
}

pub(crate) fn publish_cli(
    cli: UniversalFlags,
    cmd: PublishSubcommand,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    let cli = single_module_mooncake_cli(cli, "publish", user_log)?;
    execute_cli(
        cli,
        MooncakeSubcommands::Publish(cmd),
        &["--read-args-from-stdin"],
        "publish",
    )
}

pub(crate) fn package_cli(
    cli: UniversalFlags,
    cmd: PackageSubcommand,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    let cli = single_module_mooncake_cli(cli, "package", user_log)?;
    execute_cli(
        cli,
        MooncakeSubcommands::Package(cmd),
        &["--read-args-from-stdin"],
        "package",
    )
}

fn single_module_mooncake_cli(
    mut cli: UniversalFlags,
    command: &str,
    user_log: &UserLog,
) -> anyhow::Result<UniversalFlags> {
    let project = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(user_log)?;
    let project = project.context();
    if project.selected_module().is_none() {
        bail!(
            "`moon {command}` cannot infer a target module in workspace `{}`. Run it from a workspace member or use `moon -C <member> {command} ...`.",
            project.root().display(),
        );
    }
    cli.source_tgt_dir.cwd = None;
    Ok(cli)
}
