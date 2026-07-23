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

use clap::{Parser, Subcommand, ValueEnum};

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
    help_template = "{about-with-newline}\n{usage-heading} {usage}\n\nArguments:\n  <PACKAGE>          Registry package coordinate\n  [PROGRAM_ARGS]...  Arguments passed to the program\n\n{all-args}",
    version
)]
pub(crate) struct MoonxCli {
    #[arg(long, value_enum, default_value_t)]
    pub target: MoonxTarget,

    /// Experimental moonrun policy file; only valid for wasm
    #[arg(long = "experimental-policy", value_name = "PATH")]
    pub experimental_policy: Option<PathBuf>,

    /// Show progress and execution details
    #[arg(short = 'v', long)]
    pub verbose: bool,

    #[command(subcommand)]
    package: MoonxPackage,
}

#[derive(Debug, Subcommand)]
enum MoonxPackage {
    #[command(external_subcommand)]
    External(Vec<String>),
}

pub(crate) fn is_moonx_invocation(raw_args: &[OsString]) -> bool {
    raw_args
        .first()
        .and_then(|arg| std::path::Path::new(arg).file_name())
        .is_some_and(|name| {
            if cfg!(windows) {
                name.eq_ignore_ascii_case("moonx") || name.eq_ignore_ascii_case("moonx.exe")
            } else {
                name == "moonx" || name == "moonx.exe"
            }
        })
}

pub(crate) fn run_from_args(raw_args: &[OsString]) -> i32 {
    let cli = MoonxCli::try_parse_from(raw_args).unwrap_or_else(|err| err.exit());
    let MoonxPackage::External(package_and_args) = cli.package;
    let mut package_and_args = package_and_args.into_iter();
    let package = package_and_args
        .next()
        .expect("external subcommand always contains its name");
    let args = package_and_args.collect::<Vec<_>>();
    let args = match args.as_slice() {
        [separator, args @ ..] if separator == "--" => args.to_vec(),
        _ => args,
    };
    // moonx is a transparent runner unless the user explicitly requests details.
    let quiet = !cli.verbose;
    let output = UserDiagnostics::new(cli.verbose, quiet);
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
    match registry_runner::run(package, target, args, quiet, cli.verbose) {
        Ok(code) => code,
        Err(err) => {
            output.error(format!("{:?}", err));
            -1
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::*;

    fn invoked_as(name: &str) -> bool {
        is_moonx_invocation(&[OsString::from(name)])
    }

    #[test]
    fn recognizes_moonx_executable_names() {
        assert!(invoked_as("moonx"));
        assert!(invoked_as("moonx.exe"));
    }

    #[test]
    fn executable_name_case_matches_platform_rules() {
        assert_eq!(invoked_as("MOONX"), cfg!(windows));
        assert_eq!(invoked_as("Moonx.exe"), cfg!(windows));
    }

    #[test]
    fn rejects_moon_executable_names() {
        assert!(!invoked_as("moon"));
        assert!(!invoked_as("moon.exe"));
    }

    #[test]
    fn rejects_removed_quiet_option() {
        let error = MoonxCli::try_parse_from(["moonx", "--quiet", "user/module"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn forwards_help_and_version_flags_after_package() {
        for flag in ["-h", "--help", "-V", "--version"] {
            let cli = MoonxCli::try_parse_from(["moonx", "user/module", flag]).unwrap();
            let MoonxPackage::External(package_and_args) = cli.package;
            assert_eq!(package_and_args, ["user/module", flag]);
        }
    }
}
