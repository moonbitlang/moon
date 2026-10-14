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

//! CLI and utilities related to code coverage.

use std::{io::Write, path::Path};

use anyhow::Context;
use clap::Parser;
use moonutil::{command_output::CommandOutput, project::PackageDirs, user_log::UserLog};
use walkdir::WalkDir;

use super::{TestSubcommand, UniversalFlags, process::ProcessAction, run_test};

const MOON_COVE_REPORT_ENABLED_ENV: &str = "MOON_COVE_REPORT_ENABLED";

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
/// Runs the toolchain's `bin/moon_cove.wasm` through `moonrun` by default.
/// Set `MOON_COVE_REPORT_ENABLED=0` (or `false`) to use the legacy
/// `moon_cove_report` instead.
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
        CoverageSubcommands::Report(args) => run_coverage_report(cli, args, 0, output),
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
    // The test runtime installs handlers for these signals and Tokio leaves
    // them installed after shutdown. Preserve the incoming dispositions for
    // the final reporter, including inherited ignores such as SIG_IGN.
    #[cfg(unix)]
    let mut signal_handlers = [libc::SIGINT, libc::SIGTERM, libc::SIGQUIT].map(|signal| {
        // SAFETY: sigaction is plain C data, filled by the query below.
        (signal, unsafe { std::mem::zeroed::<libc::sigaction>() })
    });
    #[cfg(unix)]
    for (signal, handler) in &mut signal_handlers {
        // SAFETY: the signal is valid and handler is a writable sigaction.
        if unsafe { libc::sigaction(*signal, std::ptr::null(), handler) } != 0 {
            return Err(std::io::Error::last_os_error())
                .context("failed to save test signal handlers");
        }
    }

    let test_result = run_test(test_cli, test_flags, &test_output);

    // Restore even when testing returns an error. This is a terminal handoff:
    // no test runtime is started again after restoring these dispositions.
    #[cfg(unix)]
    for (signal, handler) in signal_handlers {
        // SAFETY: restore the complete disposition returned by sigaction.
        if unsafe { libc::sigaction(signal, &handler, std::ptr::null_mut()) } != 0 {
            return Err(std::io::Error::last_os_error())
                .context("failed to restore test signal handlers");
        }
    }
    let test_result = test_result?;

    let mut report_flags = CoverageReportSubcommand::default();
    report_flags.args.push("-f=simp_caret".into());
    if let Some(package) = &args.package {
        report_flags.args.push(format!("-p={package}"));
    }
    report_flags.args.extend(args.extra_flags);
    // A report is still useful after assertion failures, but a successful
    // reporter must not hide the test command's nonzero exit code.
    run_coverage_report(cli, report_flags, test_result, output)
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
    success_code: i32,
    output: &CommandOutput,
) -> anyhow::Result<ProcessAction> {
    // if help is requested, delegate to the external command
    if args.help {
        return run_coverage_reporter(
            vec!["--help".to_owned()],
            &std::env::current_dir().unwrap_or(".".into()),
            cli.dry_run,
            success_code,
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
        success_code,
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
    success_code: i32,
    error_context: &'static str,
    output: &CommandOutput,
) -> anyhow::Result<ProcessAction> {
    let use_moon_cove = std::env::var(MOON_COVE_REPORT_ENABLED_ENV).map_or(true, |value| {
        value != "0" && !value.eq_ignore_ascii_case("false")
    });

    let mut command = if use_moon_cove {
        crate::run::command_for(
            crate::run::ExecutionMode::MoonRun,
            &moonutil::toolchain::bin().join("moon_cove.wasm"),
            None,
        )
    } else {
        std::process::Command::new(&*moonutil::toolchain::BINARIES.moon_cove_report)
    };
    command
        .current_dir(cwd)
        .env_remove(MOON_COVE_REPORT_ENABLED_ENV)
        .args(args);
    if dry_run {
        output.write_result(|writer| write_coverage_report_command(writer, &command, cwd))?;
        Ok(ProcessAction::Exit(success_code))
    } else if use_moon_cove && success_code == 0 {
        Ok(ProcessAction::Delegate(command))
    } else {
        // `exec` would replace Moon and lose the test result. Wait instead
        // when that result must survive a successful report.
        Ok(ProcessAction::Run {
            command,
            success_code,
            error_context,
        })
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
