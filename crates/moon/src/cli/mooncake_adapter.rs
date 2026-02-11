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

use std::process::{Command, Stdio};

use anyhow::bail;
use moonutil::{
    cli::UniversalFlags,
    mooncakes::{
        LoginSubcommand, MooncakeSubcommands, PackageSubcommand, PublishSubcommand,
        RegisterSubcommand,
    },
};
use serde::Serialize;

pub(crate) fn execute_cli<T: Serialize>(
    cli: UniversalFlags,
    cmd: T,
    args: &[&str],
) -> anyhow::Result<i32> {
    let mut child = Command::new(&*moonutil::BINARIES.mooncake)
        .args(args)
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
        bail!("failed to run")
    }
}

pub(crate) fn execute_cli_with_inherit_stdin<T: Serialize>(
    _cli: UniversalFlags,
    _cmd: T,
    args: &[&str],
) -> anyhow::Result<i32> {
    let mut child = Command::new(&*moonutil::BINARIES.mooncake)
        .args(args)
        .env("MOONCAKE_ALLOW_DIRECT", "1")
        .stdout(Stdio::inherit())
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    let status = child.wait()?;
    if status.success() {
        Ok(0)
    } else {
        bail!("failed to run `moon {}`", args.join(" "))
    }
}

pub(crate) fn login_cli(cli: UniversalFlags, cmd: LoginSubcommand) -> anyhow::Result<i32> {
    execute_cli_with_inherit_stdin(cli, MooncakeSubcommands::Login(cmd), &["login"])
}

pub(crate) fn register_cli(cli: UniversalFlags, cmd: RegisterSubcommand) -> anyhow::Result<i32> {
    execute_cli_with_inherit_stdin(cli, MooncakeSubcommands::Register(cmd), &["register"])
}

pub(crate) fn publish_cli(cli: UniversalFlags, cmd: PublishSubcommand) -> anyhow::Result<i32> {
    execute_cli(
        cli,
        MooncakeSubcommands::Publish(cmd),
        &["--read-args-from-stdin"],
    )
}

pub(crate) fn package_cli(cli: UniversalFlags, cmd: PackageSubcommand) -> anyhow::Result<i32> {
    execute_cli(
        cli,
        MooncakeSubcommands::Package(cmd),
        &["--read-args-from-stdin"],
    )
}
