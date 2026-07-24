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

#![warn(clippy::clone_on_ref_ptr)]

use std::{
    any::Any,
    io::{IsTerminal, Write},
};

use clap::{CommandFactory, Parser};
use cli::MoonBuildSubcommands;
use moonutil::{command_output::CommandOutput, user_log::UserLog};

mod build_flags;
mod cli;
mod filter;
mod panic;
pub mod rr_build;
mod run;
#[cfg(test)]
mod tests;
mod watch;

use tracing_subscriber::{Layer, layer::SubscriberExt};

/// Initialize logging and tracing-related functionality.
///
/// This includes configuration based on multiple flags and configs:
/// - `RUST_LOG` environment variable to filter regular log output, printed to stderr.
/// - `MOON_TRACE` environment variable to enable Chrome tracing output.
/// - `--trace` CLI flag does the same thing as `MOON_TRACE=trace`, but outputs
///   to `trace.json` instead of the default `trace-<timestamp>.json`. When
///   `--trace` is used together with `MOON_TRACE`, the latter takes precedence.
///
/// Returns a boxed guard that keeps the tracing system alive.
fn init_tracing(trace_flag: bool) -> Box<dyn Any> {
    // usage example: only show debug logs for moonbuild::runtest module
    // env RUST_LOG=moonbuild::runtest=debug cargo run -- -C ./tests/test_cases/moon_new.in test

    let log_env_set = std::env::var("RUST_LOG").is_ok();
    let moon_tracing_env = std::env::var("MOON_TRACE").ok();
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::Level::WARN.into())
        .from_env_lossy();

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

    // Trace spans in Chrome format
    let chrome_trace = if trace_flag {
        // `--trace` flag
        let chrome_filter = tracing_subscriber::EnvFilter::builder()
            .with_default_directive(tracing::Level::TRACE.into())
            .parse_lossy("");
        let (layer, guard) = tracing_chrome::ChromeLayerBuilder::new()
            .include_args(true)
            .file("trace.json")
            .build();

        Some((chrome_filter.and_then(layer), guard))
    } else if let Some(env) = moon_tracing_env.as_deref() {
        // `MOON_TRACE` environment variable
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

    let fmt_layer = fmt.with_filter(filter);
    let registry = tracing_subscriber::registry()
        .with(fmt_layer)
        .with(chrome_layer);
    tracing::subscriber::set_global_default(registry)
        .expect("Failed to set global tracing subscriber");

    Box::new(chrome_guard)
}

pub fn main() {
    panic::setup_panic_hook();

    let raw_args = std::env::args_os().collect::<Vec<_>>();
    if cli::moonx::is_moonx_invocation(&raw_args) {
        std::process::exit(cli::moonx::run_from_args(&raw_args));
    }
    if cli::tool::exec::is_tool_exec(&raw_args) {
        match cli::tool::exec::run_from_raw_args(&raw_args) {
            Ok(code) => std::process::exit(code),
            Err(err) => {
                eprintln!("Error: {err:?}");
                std::process::exit(-1);
            }
        }
    }

    let cli = cli::MoonBuildCli::try_parse_from(&raw_args).unwrap_or_else(|err| {
        cli::exit_if_ide_help_request(&err, &raw_args);
        cli::exit_if_cram_external_request(&err, &raw_args);
        err.exit();
    });
    let mut flags = cli.flags;
    let subcommand = if cli.version {
        MoonBuildSubcommands::Version(cli::VersionSubcommand {
            all: true,
            json: false,
            no_path: false,
        })
    } else if let Some(subcommand) = cli.subcommand {
        subcommand
    } else {
        let mut stderr = std::io::stderr().lock();
        let _ = cli::MoonBuildCli::command().write_long_help(&mut stderr);
        let _ = writeln!(stderr);
        std::process::exit(2);
    };
    let bootstrap_output = UserLog::new(flags.user_log_level());

    if let Some(dir) = &flags.source_tgt_dir.cwd {
        // `-C` changes the process working directory early.
        if let Err(err) = std::env::set_current_dir(dir) {
            bootstrap_output.error(format!(
                "failed to change directory to {}: {}",
                dir.display(),
                err
            ));
            std::process::exit(-1);
        }
    }

    let _trace_guard = init_tracing(flags.trace);

    let (workspace_env, workspace_env_deprecation_warning) =
        match moonutil::project::current_workspace_env() {
            Ok(result) => result,
            Err(err) => {
                bootstrap_output.error(format!("{:?}", err));
                std::process::exit(-1);
            }
        };
    flags.workspace_env = workspace_env;
    let output = CommandOutput::new(flags.user_log_level(), flags.quiet);

    // Check for deprecated flags and emit warnings (after tracing is initialized)
    for warning in flags.deprecation_warnings() {
        output.user_log().warn(warning);
    }
    if let Some(warning) = workspace_env_deprecation_warning {
        output.user_log().warn(warning);
    }

    use MoonBuildSubcommands::*;
    let res = match subcommand {
        Add(a) => cli::add_cli(flags, a, output.user_log()),
        Bench(b) => cli::run_bench(flags, b, &output),
        Build(b) => cli::run_build(&flags, b, &output),
        Bundle(b) => cli::run_bundle(flags, b, &output),
        Check(c) => cli::run_check(&flags, &c, &output),
        Prove(p) => cli::run_prove(&flags, &p, &output),
        Clean(cmd) => cli::run_clean(&flags, &cmd),
        Cram(c) => cli::run_cram(&flags, c, &output),
        Coverage(c) => cli::run_coverage(flags, c, &output),
        Doc(d) => cli::run_doc(flags, d, &output),
        Fetch(f) => cli::fetch_cli(flags, f, output.user_log()),
        Work(w) => cli::work_cli(flags, w),
        Fmt(f) => cli::run_fmt(&flags, f, &output),
        GenerateBuildMatrix(b) => cli::generate_build_matrix(&flags, b),
        GenerateTestDriver(g) => cli::generate_test_driver(flags, g),
        Info(i) => cli::run_info(flags, i, &output),
        Explain(e) => cli::run_explain(&flags, e),
        Install(i) => cli::install_cli(flags, i, &output),
        Login(l) => cli::mooncake_adapter::login_cli(flags, l),
        Whoami(w) => cli::run_whoami(&flags, w),
        New(n) => cli::run_new(&flags, n, output.user_log()),
        Publish(p) => cli::mooncake_adapter::publish_cli(flags, p),
        Package(p) => cli::mooncake_adapter::package_cli(flags, p),
        Register(r) => cli::mooncake_adapter::register_cli(flags, r),
        Remove(r) => cli::remove_cli(flags, r),
        Run(r) => cli::run_run(&flags, r, &output),
        RunWasm(r) => cli::run_runwasm(&flags, r, &output),
        Test(t) => cli::run_test(flags, t, &output),
        Tree(t) => cli::tree_cli(flags, t),
        Update(u) => cli::update_cli(flags, u),
        Upgrade(u) => cli::run_upgrade(flags, u),
        ShellCompletion(gs) => cli::gen_shellcomp(&flags, gs),
        Version(v) => cli::run_version(&flags, v),
        Tool(v) => cli::run_tool(&flags, v, &output),
        External(args) => cli::run_external(args),
    };

    drop(_trace_guard);

    match res {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            output.user_log().error(format!("{:?}", e));
            std::process::exit(-1);
        }
    }
}
