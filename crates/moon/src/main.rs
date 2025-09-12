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

use clap::Parser;
use cli::MoonBuildSubcommands;

mod cli;
pub mod rr_build;
mod run;

use colored::*;
use tracing_subscriber::util::SubscriberInitExt;

fn init_log() {
    // usage example: only show debug logs for moonbuild::runtest module
    // env RUST_LOG=moonbuild::runtest=debug cargo run -- test --source-dir ./tests/test_cases/moon_new.in

    let log_env_set = std::env::var("RUST_LOG").is_ok();
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::Level::WARN.into())
        .from_env_lossy();
    let fmt = tracing_subscriber::fmt()
        .with_ansi(true)
        .with_line_number(log_env_set)
        .with_level(true)
        .with_env_filter(filter);

    let sub = if !log_env_set {
        fmt.with_target(false)
            .without_time()
            .compact()
            .set_default()
    } else {
        fmt.compact().set_default()
    };

    std::mem::forget(sub);
}

pub fn main() {
    init_log();
    match main1() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("{}: {:?}", "error".red().bold(), e);
            std::process::exit(-1);
        }
    }
}

fn main1() -> anyhow::Result<i32> {
    let cli = cli::MoonBuildCli::parse();
    let flags = cli.flags;
    use MoonBuildSubcommands::*;
    match cli.subcommand {
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
    }
}
