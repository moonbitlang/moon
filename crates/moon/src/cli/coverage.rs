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

//! CLI and utilities related to code coverage.

use std::{ffi::OsStr, path::Path};

use anyhow::Context;
use moonutil::dirs::PackageDirs;
use walkdir::WalkDir;

use super::UniversalFlags;

#[derive(Debug, clap::Parser)]
#[clap(
    allow_external_subcommands(true),
    disable_help_flag(true),
    ignore_errors(true)
)]
pub struct CoverageReportSubcommand {
    /// Arguments to pass to the coverage utility
    #[clap(name = "args", allow_hyphen_values(true))]
    pub args: Vec<String>,

    /// Show help for the coverage utility
    #[clap(short, long)]
    pub help: bool,
}

#[derive(Debug, clap::Parser)]
pub enum CoverageSubcommands {
    /// Generate code coverage report
    Report(CoverageReportSubcommand),
    /// Clean up coverage artifacts
    Clean,
}

/// Code coverage utilities
#[derive(Debug, clap::Parser)]
pub struct CoverageSubcommand {
    #[clap(subcommand)]
    pub cmd: CoverageSubcommands,
}

pub fn run_coverage(cli: UniversalFlags, cmd: CoverageSubcommand) -> anyhow::Result<i32> {
    let res = match cmd.cmd {
        CoverageSubcommands::Report(args) => run_coverage_report(cli, args),
        CoverageSubcommands::Clean => run_coverage_clean(cli),
    };
    res.context("Unable to run coverage command")
}

fn run_coverage_clean(cli: UniversalFlags) -> Result<i32, anyhow::Error> {
    let PackageDirs {
        source_dir: src,
        target_dir: tgt,
    } = cli.source_tgt_dir.try_into_package_dirs()?;
    clean_coverage_artifacts(&src, &tgt)?;
    Ok(0)
}

fn run_coverage_report(cli: UniversalFlags, args: CoverageReportSubcommand) -> anyhow::Result<i32> {
    // if help is requested, delegate to the external command
    if args.help {
        return run_coverage_report_command(
            std::iter::once("--help"),
            &std::env::current_dir().unwrap_or(".".into()),
        )
        .context("Unable to get help from coverage utility")?
        .code()
        .ok_or_else(|| anyhow::anyhow!("Unable to get exit code"));
    }

    let PackageDirs {
        source_dir: src,
        target_dir: _tgt,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let res = run_coverage_report_command(args.args, &src);
    res.context("Unable to run coverage report")?
        .code()
        .ok_or_else(|| anyhow::anyhow!("Coverage report command exited without a status code"))
}

/// Clean up coverage artifacts by removing all files with name `moonbit_coverage_*.txt` in the current directory and target
fn clean_coverage_artifacts(_src: &Path, tgt: &Path) -> anyhow::Result<()> {
    for file in WalkDir::new(tgt) {
        let file = file?;
        let file_name = file.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name.starts_with("moonbit_coverage_") && file_name.ends_with(".txt") {
            std::fs::remove_file(file.path())?;
        }
    }
    Ok(())
}

fn run_coverage_report_command(
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    cwd: &Path,
) -> std::io::Result<std::process::ExitStatus> {
    let mut cmd = std::process::Command::new("moon_cove_report");
    cmd.current_dir(cwd);
    cmd.args(args);
    cmd.status()
}
