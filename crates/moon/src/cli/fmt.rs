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

use moonbuild::execution::{BuildConfig, BuildInput};
use std::{collections::HashSet, path::PathBuf};

use anyhow::Context;
use moonbuild_rupes_recta::fmt::{FmtConfig, build_execution_plan_for_fmt_file};
use moonutil::{command_output::CommandOutput, locks::lock_directory};

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

    /// Paths to packages, files selecting their containing package, or standalone `.mbtx` scripts
    ///
    /// An explicit `.mbtx` path formats only that script, with or without a surrounding project.
    /// Multiple scripts and package paths can be combined. Without paths, standalone scripts
    /// are excluded from package formatting. `--check` fails if a selected script needs formatting.
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
    let fmt_config = FmtConfig {
        check_only: cmd.check,
        warn_only: cmd.warn,
        extra_args: cmd.args.clone(),
        migrate_moon_mod_json: cli.unstable_feature.rr_moon_mod,
        migrate_moon_pkg_json: cli.unstable_feature.rr_moon_pkg,
    };
    let (scripts, package_paths): (Vec<_>, Vec<_>) = cmd
        .path
        .iter()
        .partition(|path| path.extension().is_some_and(|ext| ext == "mbtx"));
    let script_dry_run_root = (cli.dry_run && !scripts.is_empty())
        .then(std::env::current_dir)
        .transpose()?;

    // Plan every explicit input before executing anything. Standalone scripts
    // bypass package discovery and dependency resolution, even inside a project.
    let mut plans = Vec::new();
    let mut seen_scripts = HashSet::new();
    for path in scripts {
        let script = cli.source_tgt_dir.single_file_package_dirs(path)?;
        anyhow::ensure!(
            script.file_path.is_file(),
            "formatter input `{}` must be a file",
            path.display()
        );
        if !seen_scripts.insert(script.file_path.clone()) {
            continue;
        }
        let execution_plan = build_execution_plan_for_fmt_file(
            &fmt_config,
            &script.file_path,
            &script.package_dirs.target_dir,
        )?;
        plans.push((script.package_dirs, BuildInput::new(execution_plan, None)));
    }

    if cmd.path.is_empty() || !package_paths.is_empty() {
        let dirs = cli
            .source_tgt_dir
            .query(cli.workspace_env.clone())?
            .select(user_log)?
            .package_dirs()?;
        let resolved = moonbuild_rupes_recta::fmt::resolve_for_fmt(
            &dirs.source_dir,
            &dirs.project_manifest,
            user_log,
        )
        .context("Failed to resolve environment")?;
        let selected_packages = select_packages(&package_paths, user_log, |dir| {
            filter_pkg_by_dir_for_fmt(&resolved, dir)
        })?
        .into_iter()
        .map(|(_, pkg_id)| pkg_id)
        .collect::<Vec<_>>();

        if package_paths.is_empty() || !selected_packages.is_empty() {
            let build_input = plan_fmt(
                &resolved,
                &fmt_config,
                &dirs.target_dir,
                &selected_packages,
                &dirs.project_manifest,
                user_log,
            )?;
            plans.push((dirs, build_input));
        }
    }

    for (dirs, build_input) in plans {
        if cli.dry_run {
            output.write_result(|writer| {
                rr_build::write_dry_run(
                    writer,
                    &build_input,
                    script_dry_run_root.as_deref().unwrap_or(&dirs.source_dir),
                )
            })?;
            continue;
        }
        std::fs::create_dir_all(&dirs.target_dir)?;
        let _lock = lock_directory(&dirs.target_dir, user_log)?;
        let res = moonbuild::execution::execute_build(
            &BuildConfig::default(),
            build_input,
            &dirs.target_dir,
            user_log,
        )?;
        res.print_info(cli.quiet, "formatting")?;
    }
    Ok(0)
}
