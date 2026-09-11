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
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::bail;
use mooncake::registry::{RegistryClient, RegistryRelease};
use moonutil::cli_support::{
    DeprecateSubcommand, MooncakeSubcommands, PackageSubcommand, PublishSubcommand, UniversalFlags,
};
use moonutil::command_output::CommandOutput;
use moonutil::user_log::UserLog;
use serde::Serialize;

use super::{process, search::sanitize_registry_text};

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

pub(crate) fn deprecate_cli(
    cli: UniversalFlags,
    cmd: DeprecateSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    if cli.dry_run {
        if cmd.module.contains('@') {
            bail!(
                "version-specific deprecation is not supported; specify a module without @version"
            );
        }
        // Read the same live version list as `moon view`. The preview never
        // invokes the publishing backend or sends it a mutation request.
        let manifest =
            RegistryClient::configured().module_manifest(&cmd.module.as_str().into(), None)?;
        output
            .write_result(|writer| render_deprecation_preview(writer, &cmd, &manifest.versions))?;
        Ok(0)
    } else {
        execute_cli(
            cli,
            MooncakeSubcommands::Deprecate(cmd),
            &["--read-args-from-stdin"],
            "deprecate",
        )
    }
}

pub(super) fn render_deprecation_preview(
    writer: &mut dyn Write,
    cmd: &DeprecateSubcommand,
    versions: &[RegistryRelease],
) -> std::io::Result<()> {
    let module = sanitize_registry_text(&cmd.module);
    if versions.is_empty() {
        return writeln!(writer, "No published versions found for {module}.");
    }
    for release in versions {
        if let Some(reason) = &cmd.reason {
            writeln!(
                writer,
                "Would deprecate {module}@{}: {}",
                release.version,
                sanitize_registry_text(reason)
            )?;
        } else {
            writeln!(writer, "Would restore {module}@{}", release.version)?;
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use clap::{Parser, error::ErrorKind};
    use serde_json::json;

    use super::*;
    use crate::cli::{MoonBuildCli, MoonBuildSubcommands};

    #[test]
    fn deprecate_preserves_the_private_command_payload() {
        for (args, reason, undo) in [
            (
                vec!["--reason", "  Use the replacement module instead.\n"],
                Some("  Use the replacement module instead.\n"),
                false,
            ),
            (vec!["--reason", " "], Some(" "), false),
            (vec!["--undo"], None, true),
        ] {
            let cli = MoonBuildCli::try_parse_from(
                ["moon", "deprecate", "Owner/nested/pkg?x#%2F"]
                    .into_iter()
                    .chain(args),
            )
            .unwrap();
            let Some(MoonBuildSubcommands::Deprecate(command)) = cli.subcommand else {
                panic!("expected the built-in deprecate command");
            };
            assert_eq!(
                serde_json::to_value(MooncakeSubcommands::Deprecate(command)).unwrap(),
                json!({"Deprecate": {
                    "module": "Owner/nested/pkg?x#%2F",
                    "reason": reason,
                    "undo": undo,
                }})
            );
        }
    }

    #[test]
    fn deprecation_preview_rejects_version_selectors_before_lookup() {
        let cli = MoonBuildCli::try_parse_from([
            "moon",
            "deprecate",
            "Owner/module@1.0.0",
            "--undo",
            "--dry-run",
        ])
        .unwrap();
        let Some(MoonBuildSubcommands::Deprecate(command)) = cli.subcommand else {
            panic!("expected the built-in deprecate command");
        };
        let error = deprecate_cli(
            cli.flags,
            command,
            &CommandOutput::new(log::LevelFilter::Error),
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "version-specific deprecation is not supported; specify a module without @version"
        );
    }

    #[test]
    fn deprecate_requires_a_module_and_exactly_one_action() {
        for args in [
            vec!["moon", "deprecate"],
            vec!["moon", "deprecate", "--undo"],
            vec!["moon", "deprecate", "Owner/module"],
        ] {
            assert_eq!(
                MoonBuildCli::try_parse_from(args).unwrap_err().kind(),
                ErrorKind::MissingRequiredArgument
            );
        }
        assert_eq!(
            MoonBuildCli::try_parse_from([
                "moon",
                "deprecate",
                "Owner/module",
                "--reason",
                "reason",
                "--undo",
            ])
            .unwrap_err()
            .kind(),
            ErrorKind::ArgumentConflict
        );
        assert!(
            MoonBuildCli::try_parse_from(["moon", "deprecate", "Owner/module", "--reason", ""])
                .is_err()
        );
    }
}
