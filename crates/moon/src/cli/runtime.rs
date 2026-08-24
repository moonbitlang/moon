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

use std::{
    ffi::OsString,
    io::{IsTerminal, Write},
    process::Command,
};

use anyhow::Context;
use clap::CommandFactory;
use moonutil::{
    command_output::CommandOutput,
    user_log::{UserLog, UserLogCapture, user_log_level},
};
use tracing_subscriber::{Layer, layer::SubscriberExt};

use super::{
    CheckJsonOutcome, MoonBuildCli, MoonBuildSubcommands,
    invocation::{self, DelegatedInvocation, MoonInvocation, OutputFormat, SelectedInvocation},
    process::{self, ProcessAction},
};

const INTERNAL_ERROR_EXIT_CODE: i32 = -1;

pub(crate) fn run(raw_args: Vec<OsString>) -> i32 {
    let invocation = invocation::select(raw_args).unwrap_or_else(|error| error.exit());
    match invocation {
        SelectedInvocation::Help => write_help(),
        SelectedInvocation::Moon(invocation) => run_moon(*invocation),
        SelectedInvocation::Moonx(invocation) => run_moonx(invocation),
        SelectedInvocation::Delegate(invocation) => run_transparent_delegate(invocation),
    }
}

fn write_help() -> i32 {
    let mut stderr = std::io::stderr().lock();
    let _ = MoonBuildCli::command().write_long_help(&mut stderr);
    let _ = writeln!(stderr);
    2
}

fn run_transparent_delegate(invocation: DelegatedInvocation) -> i32 {
    let result = prepare_delegate(invocation).and_then(|command| {
        process::execute(ProcessAction::Delegate(command))
            .context("failed to delegate to external command")
    });
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Error: {error:?}");
            INTERNAL_ERROR_EXIT_CODE
        }
    }
}

fn prepare_delegate(invocation: DelegatedInvocation) -> anyhow::Result<Command> {
    match invocation {
        DelegatedInvocation::ToolExec(command) => super::tool::exec::prepare(command),
        DelegatedInvocation::External { current_dir, args } => {
            super::external::prepare_external(args, current_dir.as_deref())
        }
        DelegatedInvocation::IdeHelp { current_dir, args } => super::external::prepare_external(
            std::iter::once(OsString::from("ide")).chain(args),
            current_dir.as_deref(),
        )
        .context("Unable to get help from `ide` utility"),
        DelegatedInvocation::Cram { current_dir, args } => {
            super::cram::prepare_moon_cram(current_dir.as_deref(), args)
        }
        DelegatedInvocation::Login { current_dir } => {
            super::mooncake_adapter::prepare_direct(current_dir.as_deref(), &["login"])
        }
        DelegatedInvocation::Register { current_dir } => {
            super::mooncake_adapter::prepare_direct(current_dir.as_deref(), &["register"])
        }
    }
}

fn run_moonx(invocation: super::moonx::MoonxInvocation) -> i32 {
    // moonx intentionally has no Moon tracing or workspace bootstrap. It owns
    // registry preparation, then returns the final program as a process action.
    let verbose = invocation.verbose;
    let user_log = UserLog::new(user_log_level(verbose, !verbose));
    let result = super::moonx::prepare(invocation, &user_log).and_then(process::execute);
    match result {
        Ok(code) => code,
        Err(error) => {
            user_log.error(format!("{error:?}"));
            INTERNAL_ERROR_EXIT_CODE
        }
    }
}

fn run_moon(mut invocation: MoonInvocation) -> i32 {
    let (output, capture) = match invocation.output {
        OutputFormat::Human => (CommandOutput::new(invocation.flags.user_log_level()), None),
        OutputFormat::Json => {
            let (output, capture) = CommandOutput::captured(invocation.flags.user_log_level());
            (output, Some(capture))
        }
    };

    if let Some(dir) = &invocation.flags.source_tgt_dir.cwd
        && let Err(error) = std::env::set_current_dir(dir)
    {
        let message = format!("failed to change directory to {}: {}", dir.display(), error);
        return finish_bootstrap_error(&output, capture.as_ref(), &invocation.command, message);
    }

    let trace_guard = init_tracing(
        invocation.flags.trace,
        invocation.output == OutputFormat::Json,
    );
    let (workspace_env, workspace_env_deprecation_warning) =
        match moonutil::project::current_workspace_env() {
            Ok(result) => result,
            Err(error) => {
                let message = if invocation.output == OutputFormat::Json {
                    format!("{error:#}")
                } else {
                    format!("{error:?}")
                };
                drop(trace_guard);
                return finish_bootstrap_error(
                    &output,
                    capture.as_ref(),
                    &invocation.command,
                    message,
                );
            }
        };
    invocation.flags.workspace_env = workspace_env;

    for warning in invocation.flags.deprecation_warnings() {
        output.user_log().warn(warning);
    }
    if let Some(warning) = workspace_env_deprecation_warning {
        output.user_log().warn(warning);
    }

    if invocation.output == OutputFormat::Json {
        let capture = capture
            .as_ref()
            .expect("JSON output should have a captured UserLog");
        return match &invocation.command {
            MoonBuildSubcommands::Check(command) => {
                let outcome = super::run_check_json(&invocation.flags, command, &output);
                drop(trace_guard);
                finish_check_json(&output, capture, outcome)
            }
            MoonBuildSubcommands::Search(command) => {
                let outcome = super::run_search_json(command, INTERNAL_ERROR_EXIT_CODE);
                drop(trace_guard);
                finish_search_json(&output, capture, outcome)
            }
            _ => unreachable!("command does not select JSON output"),
        };
    }

    let result = dispatch(invocation.flags, invocation.command, &output);
    drop(trace_guard);
    match result.and_then(process::execute) {
        Ok(code) => code,
        Err(error) => {
            output.user_log().error(format!("{error:?}"));
            INTERNAL_ERROR_EXIT_CODE
        }
    }
}

fn finish_bootstrap_error(
    output: &CommandOutput,
    capture: Option<&UserLogCapture>,
    command: &MoonBuildSubcommands,
    message: String,
) -> i32 {
    if let Some(capture) = capture {
        match command {
            MoonBuildSubcommands::Check(_) => finish_check_json(
                output,
                capture,
                CheckJsonOutcome::from_error(INTERNAL_ERROR_EXIT_CODE, message),
            ),
            MoonBuildSubcommands::Search(_) => finish_search_json(
                output,
                capture,
                super::SearchJsonOutcome::from_error(INTERNAL_ERROR_EXIT_CODE, message),
            ),
            _ => unreachable!("command does not select JSON output"),
        }
    } else {
        output.user_log().error(message);
        INTERNAL_ERROR_EXIT_CODE
    }
}

fn finish_search_json(
    output: &CommandOutput,
    capture: &UserLogCapture,
    outcome: super::SearchJsonOutcome,
) -> i32 {
    let exit_code = outcome.exit_code();
    if super::write_search_json(output, capture, outcome).is_err() {
        INTERNAL_ERROR_EXIT_CODE
    } else {
        exit_code
    }
}

fn finish_check_json(
    output: &CommandOutput,
    capture: &UserLogCapture,
    outcome: CheckJsonOutcome,
) -> i32 {
    let exit_code = outcome.exit_code();
    if super::write_check_json(output, capture, outcome).is_err() {
        INTERNAL_ERROR_EXIT_CODE
    } else {
        exit_code
    }
}

fn dispatch(
    flags: moonutil::cli_support::UniversalFlags,
    command: MoonBuildSubcommands,
    output: &CommandOutput,
) -> anyhow::Result<ProcessAction> {
    use MoonBuildSubcommands::*;

    match command {
        Add(command) => super::add_cli(flags, command, output.user_log()).map(Into::into),
        Bench(command) => super::run_bench(flags, command, output).map(Into::into),
        Build(command) => super::run_build(&flags, command, output).map(Into::into),
        Bundle(command) => super::run_bundle(flags, command, output).map(Into::into),
        Check(command) => super::run_check(&flags, &command, output).map(Into::into),
        Prove(command) => super::run_prove(&flags, &command, output).map(Into::into),
        Clean(command) => super::run_clean(&flags, &command, output.user_log()).map(Into::into),
        Cram(command) => super::run_cram(&flags, command, output),
        Coverage(command) => super::run_coverage(flags, command, output).map(Into::into),
        Doc(command) => super::run_doc(flags, command, output).map(Into::into),
        Fetch(command) => super::fetch_cli(flags, command, output.user_log()).map(Into::into),
        Work(command) => super::work_cli(flags, command, output.user_log()).map(Into::into),
        Fmt(command) => super::run_fmt(&flags, command, output).map(Into::into),
        GenerateBuildMatrix(command) => {
            super::generate_build_matrix(&flags, command).map(Into::into)
        }
        GenerateTestDriver(command) => super::generate_test_driver(flags, command).map(Into::into),
        Info(command) => super::run_info(flags, command, output).map(Into::into),
        Explain(command) => super::run_explain(&flags, command).map(Into::into),
        Install(command) => super::install_cli(flags, command, output.user_log()).map(Into::into),
        Whoami(command) => super::run_whoami(&flags, command).map(Into::into),
        New(command) => super::run_new(&flags, command, output.user_log()).map(Into::into),
        Publish(command) => {
            super::mooncake_adapter::publish_cli(flags, command, output.user_log()).map(Into::into)
        }
        Package(command) => {
            super::mooncake_adapter::package_cli(flags, command, output.user_log()).map(Into::into)
        }
        Remove(command) => super::remove_cli(flags, command, output.user_log()).map(Into::into),
        Run(command) => super::run_run(&flags, command, output).map(Into::into),
        RunWasm(command) => super::run_runwasm(&flags, command, output),
        Search(command) => super::run_search(command, output).map(Into::into),
        Test(command) => super::run_test(flags, command, output).map(Into::into),
        Tree(command) => super::tree_cli(flags, command, output).map(Into::into),
        Update(command) => super::update_cli(flags, command, output.user_log()).map(Into::into),
        Upgrade(command) => super::run_upgrade(flags, command).map(Into::into),
        ShellCompletion(command) => super::gen_shellcomp(&flags, command).map(Into::into),
        Version(command) => super::run_version(&flags, command).map(Into::into),
        Tool(command) => super::run_tool(&flags, command, output.user_log()).map(Into::into),
        Login(_) | Register(_) | External(_) => {
            unreachable!("transparent delegates are selected before Moon runtime setup")
        }
    }
}

/// Initialize logging and tracing-related functionality.
fn init_tracing(
    trace_flag: bool,
    suppress_terminal_output: bool,
) -> Option<tracing_chrome::FlushGuard> {
    let log_env_set = std::env::var("RUST_LOG").is_ok();
    let moon_tracing_env = std::env::var("MOON_TRACE").ok();
    let filter = if suppress_terminal_output {
        tracing_subscriber::EnvFilter::new("off")
    } else {
        tracing_subscriber::EnvFilter::builder()
            .with_default_directive(tracing::Level::WARN.into())
            .from_env_lossy()
    };

    let fmt = tracing_subscriber::fmt::layer()
        .with_ansi(std::io::stderr().is_terminal())
        .with_line_number(log_env_set)
        .with_level(true)
        .with_writer(std::io::stderr);
    let fmt = if !log_env_set {
        fmt.with_target(false).without_time().boxed()
    } else {
        fmt.compact().boxed()
    };

    let chrome_trace = if trace_flag {
        let chrome_filter = tracing_subscriber::EnvFilter::builder()
            .with_default_directive(tracing::Level::TRACE.into())
            .parse_lossy("");
        let (layer, guard) = tracing_chrome::ChromeLayerBuilder::new()
            .include_args(true)
            .file("trace.json")
            .build();
        Some((chrome_filter.and_then(layer), guard))
    } else if let Some(env) = moon_tracing_env.as_deref() {
        let chrome_filter = tracing_subscriber::EnvFilter::builder()
            .with_default_directive(tracing::Level::TRACE.into())
            .parse_lossy(env);
        let (layer, guard) = tracing_chrome::ChromeLayerBuilder::new()
            .include_args(true)
            .build();
        Some((chrome_filter.and_then(layer), guard))
    } else {
        None
    };

    let (chrome_layer, chrome_guard) = chrome_trace.unzip();
    let registry = tracing_subscriber::registry()
        .with(fmt.with_filter(filter))
        .with(chrome_layer);
    tracing::subscriber::set_global_default(registry)
        .expect("Failed to set global tracing subscriber");

    chrome_guard
}
