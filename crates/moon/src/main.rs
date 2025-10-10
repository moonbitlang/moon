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

use std::any::Any;

use clap::Parser;
use cli::MoonBuildSubcommands;

mod cli;
mod panic;
pub mod rr_build;
mod run;
mod watch;

use colored::*;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

/// Initialize logging and tracing-related functionality.
///
/// If `trace_execution` is true, it will also output the log to a Chrome
/// tracing file.
///
/// Returns a boxed guard that keeps the tracing system alive.
fn init_tracing() -> Box<dyn Any> {
    // usage example: only show debug logs for moonbuild::runtest module
    // env RUST_LOG=moonbuild::runtest=debug cargo run -- test --source-dir ./tests/test_cases/moon_new.in

    let log_env_set = std::env::var("RUST_LOG").is_ok();
    let moon_tracing_env = std::env::var("MOON_TRACE").ok();
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::Level::WARN.into())
        .from_env_lossy();

    let fmt = tracing_subscriber::fmt::layer()
        .with_ansi(true)
        .with_line_number(log_env_set)
        .with_level(true);
    let fmt = if !log_env_set {
        fmt.with_target(false).without_time().boxed()
    } else {
        fmt.compact().boxed()
    };

    // Trace spans in Chrome format

    let (chrome_layer, chrome_guard) = moon_tracing_env
        .map(|env| {
            let chrome_filter = tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::Level::TRACE.into())
                .parse_lossy(env);

            let (layer, guard) = tracing_chrome::ChromeLayerBuilder::new()
                .include_args(true)
                .build();
            (chrome_filter.and_then(layer), guard)
        })
        .unzip();

    let fmt_layer = fmt.with_filter(filter);
    let registry = tracing_subscriber::registry()
        .with(fmt_layer)
        .with(chrome_layer)
        .set_default();

    Box::new((chrome_guard, registry))
}

pub fn main() {
    panic::setup_panic_hook();

    let cli = cli::MoonBuildCli::parse();
    let flags = cli.flags;

    let _trace_guard = init_tracing();

    use MoonBuildSubcommands::*;
    let res = match cli.subcommand {
        Add(a) => cli::add_cli(flags, a),
        Bench(b) => cli::run_bench(flags, b),
        Build(b) => cli::run_build(&flags, &b),
        Bundle(b) => cli::run_bundle(flags, b),
        Check(c) => cli::run_check(&flags, &c),
        Clean(_) => cli::run_clean(&flags),
        Coverage(c) => cli::run_coverage(flags, c),
        Doc(d) => cli::run_doc(flags, d),
        Fmt(f) => cli::run_fmt(&flags, f),
        GenerateBuildMatrix(b) => cli::generate_build_matrix(&flags, b),
        GenerateTestDriver(g) => cli::generate_test_driver(flags, g),
        Info(i) => cli::run_info(flags, i),
        Install(i) => cli::install_cli(flags, i),
        Login(l) => cli::mooncake_adapter::login_cli(flags, l),
        New(n) => cli::run_new(&flags, n),
        Publish(p) => cli::mooncake_adapter::publish_cli(flags, p),
        Package(p) => cli::mooncake_adapter::package_cli(flags, p),
        Query(q) => cli::run_query(flags, q),
        Register(r) => cli::mooncake_adapter::register_cli(flags, r),
        Remove(r) => cli::remove_cli(flags, r),
        Run(r) => cli::run_run(&flags, r),
        Test(t) => cli::run_test(flags, t),
        Tree(t) => cli::tree_cli(flags, t),
        Update(u) => cli::update_cli(flags, u),
        Upgrade(u) => cli::run_upgrade(flags, u),
        ShellCompletion(gs) => cli::gen_shellcomp(&flags, gs),
        Version(v) => cli::run_version(v),
        Tool(v) => cli::run_tool(v),
        External(args) => cli::run_external(args),
    };

    drop(_trace_guard);

    match res {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("{}: {:?}", "error".red().bold(), e);
            std::process::exit(-1);
        }
    }
}
