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

use std::path::PathBuf;

use anyhow::bail;
use moonutil::constants::MOON_WORK;
use moonutil::project::WorkspaceEditTarget;
use moonutil::user_log::UserLog;

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

pub(crate) fn work_cli(
    cli: UniversalFlags,
    cmd: WorkSubcommand,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    match cmd.command {
        WorkSubcommands::Init(cmd) => {
            if cli.dry_run {
                bail!("dry-run is not supported for work init")
            }

            let workspace_root = cli.source_tgt_dir.workspace_creation_root()?;
            mooncake::pkg::init_workspace(&workspace_root, &cmd.paths, cli.quiet, user_log)
        }
        WorkSubcommands::Use(cmd) => {
            if cli.dry_run {
                bail!("dry-run is not supported for work use")
            }

            let target = cli
                .source_tgt_dir
                .workspace_edit_target(cli.workspace_env.clone(), user_log)?;
            mooncake::pkg::use_workspace(target, &cmd.paths, cli.quiet, user_log)
        }
        WorkSubcommands::Sync => {
            if cli.dry_run {
                bail!("dry-run is not supported for work sync")
            }

            let target = cli
                .source_tgt_dir
                .workspace_edit_target(cli.workspace_env.clone(), user_log)?;
            match target {
                WorkspaceEditTarget::Existing(workspace) => {
                    mooncake::pkg::sync_workspace(&workspace, cli.quiet, user_log)
                }
                WorkspaceEditTarget::CreateAt(root) => Err(anyhow::anyhow!(
                    "`moon work sync` requires `{}` at `{}`",
                    MOON_WORK,
                    root.display()
                )),
            }
        }
    }
}
