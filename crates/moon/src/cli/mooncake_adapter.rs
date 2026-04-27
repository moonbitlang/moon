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
    common::{MOON_MOD, MOON_MOD_JSON},
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
    display_name: &str,
) -> anyhow::Result<i32> {
    let current_moon = std::env::current_exe()?;
    let mut child = Command::new(&*moonutil::BINARIES.mooncake)
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

pub(crate) fn execute_cli_with_inherit_stdin<T: Serialize>(
    _cli: UniversalFlags,
    _cmd: T,
    args: &[&str],
    display_name: &str,
) -> anyhow::Result<i32> {
    let current_moon = std::env::current_exe()?;
    let mut child = Command::new(&*moonutil::BINARIES.mooncake)
        .args(args)
        .env("MOONCAKE_ALLOW_DIRECT", "1")
        .env("MOON_OVERRIDE", current_moon)
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
    let cli = single_module_mooncake_cli(cli, "publish")?;
    execute_cli(
        cli,
        MooncakeSubcommands::Publish(cmd),
        &["--read-args-from-stdin"],
        "publish",
    )
}

pub(crate) fn package_cli(cli: UniversalFlags, cmd: PackageSubcommand) -> anyhow::Result<i32> {
    let cli = single_module_mooncake_cli(cli, "package")?;
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
) -> anyhow::Result<UniversalFlags> {
    let mut query = cli.source_tgt_dir.query()?;
    let project = query.project()?;
    let module_dir = project
        .selected_module()
        .map(|module| module.root.clone())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "`moon {command}` cannot infer a target module in workspace `{}`. Run it from a workspace member or use `moon -C <member> {command} ...`.",
                project.root().display(),
            )
        })?;
    cli.source_tgt_dir.cwd = None;
    cli.source_tgt_dir.manifest_path = Some(if module_dir.join(MOON_MOD).exists() {
        module_dir.join(MOON_MOD)
    } else {
        module_dir.join(MOON_MOD_JSON)
    });
    Ok(cli)
}

#[cfg(test)]
mod tests {
    use super::single_module_mooncake_cli;
    use moonutil::{
        cli::UniversalFlags,
        common::{MOON_MOD, MOON_MOD_JSON},
        dirs::SourceTargetDirs,
    };
    use std::path::Path;

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn single_module_mooncake_cli_targets_the_selected_member_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let member = root.join("app");
        write_file(&root.join("moon.work"), "members = [\n  \"./app\",\n]\n");
        write_file(
            &member.join(MOON_MOD),
            "name = \"alice/app\"\n\nversion = \"0.1.0\"\n",
        );

        let cli = UniversalFlags {
            source_tgt_dir: SourceTargetDirs {
                cwd: None,
                manifest_path: Some(member.join(MOON_MOD)),
                target_dir: None,
            },
            quiet: false,
            verbose: false,
            trace: false,
            dry_run: false,
            build_graph: false,
            unstable_feature: Box::default(),
        };

        let cli = single_module_mooncake_cli(cli, "package").unwrap();
        let actual_manifest_path = cli
            .source_tgt_dir
            .manifest_path
            .as_ref()
            .map(dunce::canonicalize)
            .unwrap()
            .unwrap();
        let expected_manifest_path = member.join(MOON_MOD);

        assert_eq!(cli.source_tgt_dir.cwd, None);
        assert_eq!(
            actual_manifest_path,
            dunce::canonicalize(expected_manifest_path).unwrap()
        );
        assert_eq!(cli.source_tgt_dir.target_dir, None);
    }

    #[test]
    fn single_module_mooncake_cli_falls_back_to_json_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let member = root.join("app");
        write_file(&root.join("moon.work"), "members = [\n  \"./app\",\n]\n");
        write_file(
            &member.join(MOON_MOD_JSON),
            "{\n  \"name\": \"alice/app\",\n  \"version\": \"0.1.0\"\n}\n",
        );

        let cli = UniversalFlags {
            source_tgt_dir: SourceTargetDirs {
                cwd: None,
                manifest_path: Some(member.join(MOON_MOD_JSON)),
                target_dir: None,
            },
            quiet: false,
            verbose: false,
            trace: false,
            dry_run: false,
            build_graph: false,
            unstable_feature: Box::default(),
        };

        let cli = single_module_mooncake_cli(cli, "package").unwrap();
        let actual_manifest_path = cli
            .source_tgt_dir
            .manifest_path
            .as_ref()
            .map(dunce::canonicalize)
            .unwrap()
            .unwrap();
        let expected_manifest_path = member.join(MOON_MOD_JSON);

        assert_eq!(cli.source_tgt_dir.cwd, None);
        assert_eq!(
            actual_manifest_path,
            dunce::canonicalize(expected_manifest_path).unwrap()
        );
        assert_eq!(cli.source_tgt_dir.target_dir, None);
    }
}
