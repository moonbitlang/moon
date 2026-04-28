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

use std::path::PathBuf;

use anyhow::bail;
use moonutil::dirs::PackageDirs;

use super::UniversalFlags;

/// Workspace maintenance commands
#[derive(Debug, clap::Parser)]
pub(crate) struct WorkSubcommand {
    #[clap(subcommand)]
    command: WorkSubcommands,
}

#[derive(Debug, clap::Parser)]
enum WorkSubcommands {
    /// Create a workspace manifest
    Init(WorkInitSubcommand),
    /// Add modules to the workspace manifest
    Use(WorkUseSubcommand),
    /// Sync workspace dependency versions into member manifests
    Sync,
}

#[derive(Debug, clap::Parser)]
pub(crate) struct WorkInitSubcommand {
    /// Module directories to add to the workspace
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, clap::Parser)]
pub(crate) struct WorkUseSubcommand {
    /// Module directories to add to the workspace
    #[clap(required = true)]
    pub paths: Vec<PathBuf>,
}

pub(crate) fn work_cli(cli: UniversalFlags, cmd: WorkSubcommand) -> anyhow::Result<i32> {
    match cmd.command {
        WorkSubcommands::Init(cmd) => {
            if cli.dry_run {
                bail!("dry-run is not supported for work init")
            }

            let workspace_root = work_root(&cli, false)?;
            mooncake::pkg::init_workspace(&workspace_root, &cmd.paths, cli.quiet)
        }
        WorkSubcommands::Use(cmd) => {
            if cli.dry_run {
                bail!("dry-run is not supported for work use")
            }

            let workspace_root = work_root(&cli, true)?;
            mooncake::pkg::use_workspace(&workspace_root, &cmd.paths, cli.quiet)
        }
        WorkSubcommands::Sync => {
            if cli.dry_run {
                bail!("dry-run is not supported for work sync")
            }

            let PackageDirs { source_dir, .. } = cli
                .source_tgt_dir
                .query(cli.workspace_env.clone())?
                .package_dirs()?;
            mooncake::pkg::sync_workspace(&source_dir, cli.quiet)
        }
    }
}

fn work_root(cli: &UniversalFlags, prefer_existing_workspace: bool) -> anyhow::Result<PathBuf> {
    Ok(cli
        .source_tgt_dir
        .work_root(prefer_existing_workspace, cli.workspace_env.clone())?)
}
