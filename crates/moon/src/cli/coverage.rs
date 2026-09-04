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

use std::{io::Write, path::Path};

use anyhow::Context;
use clap::Parser;
use moonutil::{command_output::CommandOutput, project::PackageDirs, user_log::UserLog};
use walkdir::WalkDir;

use super::{
    TestSubcommand, UniversalFlags,
    moonx::{self, MoonxInvocation},
    process::ProcessAction,
    run_test,
};

const MOON_COVE_PACKAGE: &str = "moonbitlang/moon_cove";
const DEFAULT_MOON_COVE_REPORT_VERSION: &str = "0.3.1";
const MOON_COVE_REPORT_ENABLED_ENV: &str = "MOON_COVE_REPORT_ENABLED";
const MOON_COVE_REPORT_VERSION_ENV: &str = "MOON_COVE_REPORT_VERSION";

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
///
/// Set `MOON_COVE_REPORT_ENABLED=1` (or `true`) to run
/// `moonbitlang/moon_cove` through `moonx`. `MOON_COVE_REPORT_VERSION`
/// optionally selects its version and defaults to 0.3.1. When disabled, Moon
/// uses the toolchain's `moon_cove_report`.
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

    /// Extra flags passed directly to the selected coverage reporter
    #[arg(last = true, global = true, name = "EXTRA_FLAGS")]
    extra_flags: Vec<String>,
}

pub(crate) fn run_coverage(
    cli: UniversalFlags,
    cmd: CoverageSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<ProcessAction> {
    let res = match cmd.cmd {
        CoverageSubcommands::Analyze(args) => run_coverage_analyze(cli, args, output),
        CoverageSubcommands::Report(args) => run_coverage_report(cli, args, output),
        CoverageSubcommands::Clean => {
            run_coverage_clean(cli, output.user_log()).map(ProcessAction::from)
        }
    };
    res.context("Unable to run coverage command")
}

fn run_coverage_analyze(
    cli: UniversalFlags,
    args: CoverageAnalyzeSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<ProcessAction> {
    run_coverage_clean(cli.clone(), output.user_log())?;

    let mut test_args = vec!["test".to_owned()];
    test_args.extend(args.test_flag);
    let mut test_flags = TestSubcommand::try_parse_from(test_args)?;
    test_flags.build_flags.enable_coverage = true;
    let test_cli = UniversalFlags {
        quiet: true, // Disable output for `moon test` on success
        ..cli.clone()
    };
    let test_output = CommandOutput::new(test_cli.user_log_level());
    run_test(test_cli, test_flags, &test_output)?;

    let mut report_flags = CoverageReportSubcommand::default();
    report_flags.args.push("-f=simp_caret".into());
    if let Some(package) = &args.package {
        report_flags.args.push(format!("-p={package}"));
    }
    report_flags.args.extend(args.extra_flags);
    run_coverage_report(cli, report_flags, output)
}

fn run_coverage_clean(cli: UniversalFlags, user_log: &UserLog) -> Result<i32, anyhow::Error> {
    let PackageDirs {
        source_dir: src,
        target_dir: tgt,
        ..
    } = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(user_log)?
        .package_dirs()?;
    clean_coverage_artifacts(&src, &tgt)?;
    Ok(0)
}

fn run_coverage_report(
    cli: UniversalFlags,
    args: CoverageReportSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<ProcessAction> {
    // if help is requested, delegate to the external command
    if args.help {
        return run_coverage_reporter(
            vec!["--help".to_owned()],
            &std::env::current_dir().unwrap_or(".".into()),
            cli.dry_run,
            "Unable to get help from coverage utility",
            output,
        );
    }

    let PackageDirs {
        source_dir: src,
        target_dir: _tgt,
        ..
    } = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(output.user_log())?
        .package_dirs()?;

    run_coverage_reporter(
        args.args,
        &src,
        cli.dry_run,
        "Unable to run coverage report",
        output,
    )
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

fn run_coverage_reporter(
    args: Vec<String>,
    cwd: &Path,
    dry_run: bool,
    error_context: &'static str,
    output: &CommandOutput,
) -> anyhow::Result<ProcessAction> {
    let use_moon_cove = std::env::var_os(MOON_COVE_REPORT_ENABLED_ENV)
        .and_then(|value| value.into_string().ok())
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));

    if use_moon_cove {
        let version = std::env::var(MOON_COVE_REPORT_VERSION_ENV)
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_MOON_COVE_REPORT_VERSION.to_owned());
        let coordinate = format!("{MOON_COVE_PACKAGE}@{version}");
        
        if dry_run {
            let mut command = vec![
                "moonx".to_owned(),
                "--target".to_owned(),
                "wasm".to_owned(),
                coordinate,
                "--".to_owned(),
            ];
            command.extend(args);
            output.write_result(|writer| {
                writeln!(writer, "(cd {} && {})", cwd.display(), command.join(" "))
            })?;
            return Ok(ProcessAction::Exit(0));
        }

        let user_log = output.user_log().with_level(log::LevelFilter::Warn);
        let mut action = moonx::prepare(MoonxInvocation::wasm_package(coordinate, args), &user_log)
            .context(error_context)?;
        match &mut action {
            ProcessAction::Delegate(command)
            | ProcessAction::DelegateWithPolicyRelay(command, _) => {
                command
                    .current_dir(cwd)
                    .env_remove(MOON_COVE_REPORT_ENABLED_ENV)
                    .env_remove(MOON_COVE_REPORT_VERSION_ENV);
            }
            ProcessAction::Exit(_) => {}
        }
        return Ok(action);
    }

    let mut command = std::process::Command::new(&*moonutil::toolchain::BINARIES.moon_cove_report);
    command
        .current_dir(cwd)
        .env_remove(MOON_COVE_REPORT_ENABLED_ENV)
        .env_remove(MOON_COVE_REPORT_VERSION_ENV)
        .args(args);
    if dry_run {
        output.write_result(|writer| write_coverage_report_command(writer, &command, cwd))?;
        Ok(ProcessAction::Exit(0))
    } else {
        let code = command
            .status()
            .context(error_context)?
            .code()
            .ok_or_else(|| {
                anyhow::anyhow!("Coverage report command exited without a status code")
            })?;
        Ok(ProcessAction::Exit(code))
    }
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
