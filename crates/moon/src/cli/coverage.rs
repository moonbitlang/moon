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

use std::{ffi::OsStr, io::Write, path::Path};

use anyhow::Context;
use clap::Parser;
use moonutil::{command_output::CommandOutput, project::PackageDirs};
use walkdir::WalkDir;

use super::{TestSubcommand, UniversalFlags, run_test};

#[derive(Debug, clap::Parser, Default)]
#[clap(
    allow_external_subcommands(true),
    disable_help_flag(true),
    ignore_errors(true)
)]
pub(crate) struct CoverageReportSubcommand {
    /// Arguments to pass to the coverage utility
    #[clap(name = "args", allow_hyphen_values(true))]
    pub args: Vec<String>,

    /// Show help for the coverage utility
    #[clap(short, long)]
    pub help: bool,
}

#[derive(Debug, clap::Parser)]
pub(crate) enum CoverageSubcommands {
    /// Run test with instrumentation and report coverage
    Analyze(CoverageAnalyzeSubcommand),
    /// Generate code coverage report
    Report(CoverageReportSubcommand),
    /// Clean up coverage artifacts
    Clean,
}

/// Code coverage utilities
#[derive(Debug, clap::Parser)]
pub(crate) struct CoverageSubcommand {
    #[clap(subcommand)]
    pub cmd: CoverageSubcommands,
}

#[derive(Debug, clap::Parser)]
pub(crate) struct CoverageAnalyzeSubcommand {
    /// Analyze coverage for a specific package.
    #[clap(short, long)]
    package: Option<String>,

    /// Extra flags passed directly to `moon test`
    #[clap(short, long, hide = true, allow_hyphen_values = true)]
    pub test_flag: Vec<String>,

    /// Extra flags passed directly to `moon_cove_report`
    #[arg(last = true, global = true, name = "EXTRA_FLAGS")]
    extra_flags: Vec<String>,
}

pub(crate) fn run_coverage(
    cli: UniversalFlags,
    cmd: CoverageSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let res = match cmd.cmd {
        CoverageSubcommands::Analyze(args) => run_coverage_analyze(cli, args, output),
        CoverageSubcommands::Report(args) => run_coverage_report(cli, args, output),
        CoverageSubcommands::Clean => run_coverage_clean(cli),
    };
    res.context("Unable to run coverage command")
}

fn run_coverage_analyze(
    cli: UniversalFlags,
    args: CoverageAnalyzeSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    run_coverage_clean(cli.clone())?;

    let mut test_args = vec!["test".to_owned()];
    test_args.extend(args.test_flag);
    let mut test_flags = TestSubcommand::try_parse_from(test_args)?;
    test_flags.build_flags.enable_coverage = true;
    let test_cli = UniversalFlags {
        quiet: true, // Disable output for `moon test` on success
        ..cli.clone()
    };
    let test_output = CommandOutput::new(test_cli.user_log_level(), test_cli.quiet);
    run_test(test_cli, test_flags, &test_output)?;

    let mut report_flags = CoverageReportSubcommand::default();
    report_flags.args.push("-f=simp_caret".into());
    if let Some(package) = &args.package {
        report_flags.args.push(format!("-p={package}"));
    }
    report_flags.args.extend(args.extra_flags);
    run_coverage_report(cli, report_flags, output)
}

fn run_coverage_clean(cli: UniversalFlags) -> Result<i32, anyhow::Error> {
    let PackageDirs {
        source_dir: src,
        target_dir: tgt,
        ..
    } = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .package_dirs()?;
    clean_coverage_artifacts(&src, &tgt)?;
    Ok(0)
}

fn run_coverage_report(
    cli: UniversalFlags,
    args: CoverageReportSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    // if help is requested, delegate to the external command
    if args.help {
        return coverage_report_command(
            std::iter::once("--help"),
            &std::env::current_dir().unwrap_or(".".into()),
        )
        .status()
        .context("Unable to get help from coverage utility")?
        .code()
        .ok_or_else(|| anyhow::anyhow!("Unable to get exit code"));
    }

    let PackageDirs {
        source_dir: src,
        target_dir: _tgt,
        ..
    } = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .package_dirs()?;

    let mut command = coverage_report_command(args.args, &src);
    if cli.dry_run {
        output.write_result(|writer| write_coverage_report_command(writer, &command, &src))?;
        return Ok(0);
    }
    command
        .status()
        .context("Unable to run coverage report")?
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

fn coverage_report_command(
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    cwd: &Path,
) -> std::process::Command {
    let mut cmd = std::process::Command::new(&*moonutil::toolchain::BINARIES.moon_cove_report);
    cmd.current_dir(cwd);
    cmd.args(args);
    cmd
}

fn write_coverage_report_command(
    output: &mut dyn Write,
    command: &std::process::Command,
    cwd: &Path,
) -> std::io::Result<()> {
    let args = std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|s| s.to_string_lossy())
        .collect::<Vec<_>>();
    writeln!(output, "(cd {} && {})", cwd.display(), args.join(" "))
}
