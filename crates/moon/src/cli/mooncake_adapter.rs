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
    path::PathBuf,
    process::{Command, Stdio},
};

use anyhow::bail;
use moonutil::{
    cli::UniversalFlags,
    common::{MOON_MOD_JSON, MOON_WORK, MOON_WORK_JSON},
    mooncakes::{
        LoginSubcommand, MooncakeSubcommands, PackageSubcommand, PublishSubcommand,
        RegisterSubcommand,
    },
    workspace::{read_workspace, write_workspace_legacy_json},
};
use serde::Serialize;

pub(crate) fn execute_cli<T: Serialize>(
    cli: UniversalFlags,
    cmd: T,
    args: &[&str],
    display_name: &str,
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
        bail!("`moon {}` failed", display_name)
    }
}

pub(crate) fn execute_cli_with_inherit_stdin<T: Serialize>(
    _cli: UniversalFlags,
    _cmd: T,
    args: &[&str],
    display_name: &str,
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
        bail!("`moon {}` failed", display_name)
    }
}

pub(crate) fn login_cli(cli: UniversalFlags, cmd: LoginSubcommand) -> anyhow::Result<i32> {
    execute_cli_with_inherit_stdin(cli, MooncakeSubcommands::Login(cmd), &["login"], "login")
}

pub(crate) fn register_cli(cli: UniversalFlags, cmd: RegisterSubcommand) -> anyhow::Result<i32> {
    execute_cli_with_inherit_stdin(
        cli,
        MooncakeSubcommands::Register(cmd),
        &["register"],
        "register",
    )
}

pub(crate) fn publish_cli(cli: UniversalFlags, cmd: PublishSubcommand) -> anyhow::Result<i32> {
    let (cli, compat_workspace) = single_module_mooncake_cli(cli, "publish")?;
    execute_cli(
        cli,
        MooncakeSubcommands::Publish(cmd),
        &["--read-args-from-stdin"],
        "publish",
    )?;
    drop(compat_workspace);
    Ok(0)
}

pub(crate) fn package_cli(cli: UniversalFlags, cmd: PackageSubcommand) -> anyhow::Result<i32> {
    let (cli, compat_workspace) = single_module_mooncake_cli(cli, "package")?;
    execute_cli(
        cli,
        MooncakeSubcommands::Package(cmd),
        &["--read-args-from-stdin"],
        "package",
    )?;
    drop(compat_workspace);
    Ok(0)
}

fn single_module_mooncake_cli(
    mut cli: UniversalFlags,
    command: &str,
) -> anyhow::Result<(UniversalFlags, Option<LegacyWorkspaceCompatFile>)> {
    let dirs = cli.source_tgt_dir.try_into_workspace_module_dirs()?;
    let module_dir = dirs.require_module_dir(command)?;
    let compat_workspace = LegacyWorkspaceCompatFile::ensure_for_workspace(&dirs.project_root)?;
    cli.source_tgt_dir.cwd = None;
    cli.source_tgt_dir.manifest_path = Some(module_dir.join(MOON_MOD_JSON));
    Ok((cli, compat_workspace))
}

struct LegacyWorkspaceCompatFile {
    path: PathBuf,
}

impl LegacyWorkspaceCompatFile {
    fn ensure_for_workspace(project_root: &std::path::Path) -> anyhow::Result<Option<Self>> {
        let dsl_path = project_root.join(MOON_WORK);
        let legacy_path = project_root.join(MOON_WORK_JSON);
        if !dsl_path.exists() || legacy_path.exists() {
            return Ok(None);
        }

        let Some(workspace) = read_workspace(project_root)? else {
            return Ok(None);
        };
        write_workspace_legacy_json(project_root, &workspace)?;
        Ok(Some(Self { path: legacy_path }))
    }
}

impl Drop for LegacyWorkspaceCompatFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
