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

use moonbuild::execution::BuildConfig;
use std::path::PathBuf;

use anyhow::Context;
use moonbuild_rupes_recta::fmt::FmtConfig;
use moonutil::{command_output::CommandOutput, locks::lock_directory, project::PackageDirs};

use crate::filter::{filter_pkg_by_dir_for_fmt, select_packages};
use crate::rr_build::{self, plan_fmt};

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

    /// Warn if code is not properly formatted
    #[clap(long, conflicts_with = "check")]
    pub warn: bool,

    /// Paths to package directories or files inside packages to format
    #[clap(name = "PATH")]
    pub path: Vec<PathBuf>,

    /// Extra arguments passed to the formatter (after --)
    #[clap(last = true)]
    pub args: Vec<String>,
}

pub(crate) fn run_fmt(
    cli: &UniversalFlags,
    cmd: FmtSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    run_fmt_rr(cli, cmd, output)
}

fn run_fmt_rr(
    cli: &UniversalFlags,
    cmd: FmtSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let user_log = output.user_log();
    let PackageDirs {
        source_dir,
        target_dir,
        project_manifest,
        ..
    } = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(user_log)?
        .package_dirs()?;

    let resolved =
        moonbuild_rupes_recta::fmt::resolve_for_fmt(&source_dir, &project_manifest, user_log)
            .context("Failed to resolve environment")?;

    let mut selected_packages = Vec::new();

    for (_, pkg_id) in select_packages(&cmd.path, user_log, |dir| {
        filter_pkg_by_dir_for_fmt(&resolved, dir)
    })? {
        selected_packages.push(pkg_id);
    }

    if !cmd.path.is_empty() && selected_packages.is_empty() {
        return Ok(0);
    }

    let fmt_config = FmtConfig {
        check_only: cmd.check,
        warn_only: cmd.warn,
        extra_args: cmd.args.clone(),
        migrate_moon_mod_json: cli.unstable_feature.rr_moon_mod,
        migrate_moon_pkg_json: cli.unstable_feature.rr_moon_pkg,
    };
    let build_input = plan_fmt(
        &resolved,
        &fmt_config,
        &target_dir,
        &selected_packages,
        &project_manifest,
        user_log,
    )?;

    if cli.dry_run {
        output.write_result(|writer| rr_build::write_dry_run(writer, &build_input, &source_dir))?;
        Ok(0)
    } else {
        std::fs::create_dir_all(&target_dir)?;
        let _lock = lock_directory(&target_dir, user_log)?;
        let res = moonbuild::execution::execute_build(
            &BuildConfig::default(),
            build_input,
            &target_dir,
            user_log,
        )?;
        res.print_info(cli.quiet, "formatting")?;
        Ok(res.return_code_for_success())
    }
}
