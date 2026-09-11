// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
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
