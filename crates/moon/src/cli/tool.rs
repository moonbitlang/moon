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

pub(crate) mod build_binary_dep;
pub(crate) mod demangle;
pub(crate) mod embed;
pub(crate) mod exec;
pub(crate) mod format_and_diff;
pub(crate) mod format_workspace;
pub(crate) mod generate_node_test_package_config;
pub(crate) mod migrate_manifest;

use demangle::*;
use embed::*;
use format_and_diff::*;
use format_workspace::*;
use generate_node_test_package_config::*;
use migrate_manifest::*;
use moonutil::{cli_support::UniversalFlags, user_log::UserLog};

#[derive(Debug, clap::Parser)]
pub(crate) struct ToolSubcommand {
    #[clap(subcommand)]
    pub subcommand: ToolSubcommands,
}

#[derive(Debug, clap::Parser)]
pub(crate) enum ToolSubcommands {
    FormatAndDiff(FormatAndDiffSubcommand),
    FormatWorkspace(FormatWorkspaceSubcommand),
    #[clap(hide = true)]
    GenerateNodeTestPackageConfig(GenerateNodeTestPackageConfigSubcommand),
    #[clap(hide = true)]
    MigrateManifest(MigrateManifestSubcommand),
    Embed(Embed),
    BuildBinaryDep(build_binary_dep::BuildBinaryDepArgs),
    Demangle(DemangleSubcommand),
}

pub(crate) fn run_tool(
    cli: &UniversalFlags,
    cmd: ToolSubcommand,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    match cmd.subcommand {
        ToolSubcommands::FormatAndDiff(subcmd) => run_format_and_diff(subcmd),
        ToolSubcommands::FormatWorkspace(subcmd) => run_format_workspace(subcmd, user_log),
        ToolSubcommands::GenerateNodeTestPackageConfig(subcmd) => {
            generate_node_test_package_config(cli, subcmd)
        }
        ToolSubcommands::MigrateManifest(subcmd) => run_migrate_manifest(subcmd),
        ToolSubcommands::Embed(subcmd) => run_embed(subcmd),
        ToolSubcommands::BuildBinaryDep(subcmd) => {
            build_binary_dep::run_build_binary_dep(cli, &subcmd, user_log)
        }
        ToolSubcommands::Demangle(subcmd) => Ok(run_demangle(subcmd)),
    }
}
