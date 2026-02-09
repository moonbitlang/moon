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

use anyhow::Context;
use moonbuild_rupes_recta::fmt::FmtConfig;
use moonutil::{common::BlockStyle, dirs::PackageDirs};

use crate::filter::{canonicalize_with_filename, filter_pkg_by_dir_for_fmt};
use crate::rr_build::{self, BuildConfig, plan_fmt};

use super::UniversalFlags;

/// Format source code
#[derive(Debug, clap::Parser)]
pub(crate) struct FmtSubcommand {
    /// Check only and don't change the source code
    #[clap(long)]
    pub check: bool,

    /// Sort input files
    #[clap(long)]
    pub sort_input: bool,

    /// Add separator between each segments
    #[clap(long, value_enum, num_args=0..=1, default_missing_value = "true")]
    pub block_style: Option<BlockStyle>,

    /// Warn if code is not properly formatted
    #[clap(long, conflicts_with = "check")]
    pub warn: bool,

    /// Path to a package directory to format
    #[clap(name = "PATH")]
    pub path: Option<PathBuf>,

    /// Extra arguments passed to the formatter (after --)
    #[clap(last = true)]
    pub args: Vec<String>,
}

pub fn run_fmt(cli: &UniversalFlags, cmd: FmtSubcommand) -> anyhow::Result<i32> {
    run_fmt_rr(cli, cmd)
}

fn run_fmt_rr(cli: &UniversalFlags, cmd: FmtSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let resolved = moonbuild_rupes_recta::fmt::resolve_for_fmt(&source_dir)
        .context("Failed to resolve environment")?;

    // Resolve the package filter from the path argument
    let package_filter = if let Some(path) = &cmd.path {
        let (dir, _) = canonicalize_with_filename(path)
            .with_context(|| format!("Cannot canonicalize provided path '{}'", path.display()))?;
        Some(filter_pkg_by_dir_for_fmt(&resolved, &dir)?)
    } else {
        None
    };

    let fmt_config = FmtConfig {
        block_style: cmd.block_style.unwrap_or_default().is_line(),
        check_only: cmd.check,
        warn_only: cmd.warn,
        extra_args: cmd.args.clone(),
        migrate_moon_pkg_json: cli.unstable_feature.rr_moon_pkg,
    };
    let graph = plan_fmt(&resolved, &fmt_config, &target_dir, package_filter)?;

    if cli.dry_run {
        rr_build::print_dry_run_all(&graph, &source_dir, &target_dir);
        Ok(0)
    } else {
        let res = rr_build::execute_build(&BuildConfig::default(), graph, &target_dir)?;
        res.print_info(cli.quiet, "formatting")?;
        Ok(res.return_code_for_success())
    }
}
