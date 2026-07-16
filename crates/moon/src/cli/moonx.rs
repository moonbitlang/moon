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

use std::{ffi::OsString, path::PathBuf};

use clap::{Parser, ValueEnum};

use crate::user_diagnostics::UserDiagnostics;

use super::registry_runner::{self, RegistryRunTarget};

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum MoonxTarget {
    #[default]
    Wasm,
    Native,
}

#[derive(Debug, Parser)]
#[command(
    name = "moonx",
    about = "Run a package from the Mooncakes registry without installing it",
    override_usage = "moonx [OPTIONS] <PACKAGE> [PROGRAM_ARGS]...",
    version
)]
pub(crate) struct MoonxCli {
    /// Registry package coordinate
    #[arg(value_name = "PACKAGE")]
    pub package: String,

    #[arg(long, value_enum, default_value_t)]
    pub target: MoonxTarget,

    /// Experimental moonrun policy file; only valid for wasm
    #[arg(long = "experimental-policy", value_name = "PATH")]
    pub experimental_policy: Option<PathBuf>,

    /// Suppress output
    #[arg(short = 'q', long)]
    pub quiet: bool,

    /// Increase verbosity
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Arguments passed to the program
    #[arg(
        value_name = "PROGRAM_ARGS",
        trailing_var_arg = true,
        allow_hyphen_values = true
    )]
    pub args: Vec<String>,
}

pub(crate) fn is_moonx_invocation(raw_args: &[OsString]) -> bool {
    raw_args
        .first()
        .and_then(|arg| std::path::Path::new(arg).file_name())
        .is_some_and(|name| name == "moonx" || name == "moonx.exe")
}

pub(crate) fn run_from_args(raw_args: &[OsString]) -> i32 {
    let cli = MoonxCli::try_parse_from(raw_args).unwrap_or_else(|err| err.exit());
    let output = UserDiagnostics::new(cli.verbose, cli.quiet);
    let target = match cli.target {
        MoonxTarget::Wasm => RegistryRunTarget::Wasm {
            experimental_policy: cli.experimental_policy,
        },
        MoonxTarget::Native if cli.experimental_policy.is_some() => {
            output.error("--experimental-policy is only valid with `--target wasm`");
            return -1;
        }
        MoonxTarget::Native => RegistryRunTarget::Native,
    };
    match registry_runner::run(cli.package, target, cli.args, cli.quiet, cli.verbose) {
        Ok(code) => code,
        Err(err) => {
            output.error(format!("{:?}", err));
            -1
        }
    }
}
