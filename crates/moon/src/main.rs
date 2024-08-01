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

use colored::*;

fn init_log() {
    use std::io::Write;
    // usage example: only show debug logs for moonbuild::runtest module
    // env RUST_LOG=moonbuild::runtest=debug cargo run -- test --source-dir ./tests/test_cases/moon_new.in

    // log level: error > warn > info > debug > trace
    env_logger::Builder::from_env(env_logger::Env::default())
        .target(env_logger::Target::Stdout)
        .format(|buf, record| {
            let level_style = buf.default_level_style(record.level());
            writeln!(
                buf,
                "{} [{}] [{}:{}] {}",
                level_style.value(record.level()),
                chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .init();
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
        Register(r) => cli::mooncake_adapter::register_cli(flags, r),
        Remove(r) => cli::remove_cli(flags, r),
        Run(r) => cli::run_run(&flags, r),
        Test(t) => cli::run_test(flags, t),
        Tree(t) => cli::tree_cli(flags, t),
        Update(u) => cli::update_cli(flags, u),
        Upgrade => cli::run_upgrade(flags),
        ShellCompletion(gs) => cli::gen_shellcomp(&flags, gs),
        Version(v) => cli::run_version(v),
    }
}
