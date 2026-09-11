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

use anyhow::{Context, bail};
use moonutil::{cli_support::UniversalFlags, locks::lock_directory, user_log::UserLog};

/// Remove local build outputs or configured global caches.
#[derive(Debug, clap::Parser)]
pub(crate) struct CleanSubcommand {
    /// Remove the global dependency-source cache instead of `_build`.
    #[arg(long)]
    dep_cache: bool,

    /// Remove the global build-artifact cache instead of `_build`.
    #[arg(long)]
    build_cache: bool,
}

pub(crate) fn run_clean(
    cli: &UniversalFlags,
    cmd: &CleanSubcommand,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("dry-run is not supported for clean");
    }

    if cmd.dep_cache || cmd.build_cache {
        if cmd.dep_cache {
            moonutil::cache::clean_cache(moonutil::cache::CacheKind::DependencySources)?;
        }
        if cmd.build_cache {
            moonutil::cache::clean_cache(moonutil::cache::CacheKind::BuildArtifacts)?;
        }
        return Ok(0);
    }

    let src_tgt = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(user_log)?
        .package_dirs()?;

    let _lock = lock_directory(&src_tgt.target_dir, user_log)?;

    if src_tgt.target_dir.is_dir() {
        std::fs::remove_dir_all(&src_tgt.target_dir)
            .context("failed to remove _build directory")?;
    }

    Ok(0)
}
