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
use moonutil::user_log::UserLog;

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
    long_about = r#"Run a package from the Mooncakes registry without installing it.

Accepted package coordinate forms:
  moonx user/module/package
  moonx user/module/package@1.2.3
  moonx user/module/package@latest

Pinned coordinates use the requested version directly. `@latest` refreshes the
registry index before resolving the latest version. Unpinned coordinates use
the latest version already known to the local registry index."#,
    override_usage = "moonx [OPTIONS] <PACKAGE> [PROGRAM_ARGS]...",
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

#[derive(Debug)]
pub(crate) struct MoonxInvocation {
    pub(crate) target: MoonxTarget,
    pub(crate) experimental_policy: Option<PathBuf>,
    pub(crate) verbose: bool,
    inherited_policy: Option<OsString>,
    package: String,
    args: Vec<String>,
}

pub(crate) fn is_moonx_invocation(raw_args: &[OsString]) -> bool {
    raw_args
        .first()
        .is_some_and(|arg| moonutil::constants::is_moonx_executable(arg))
}

pub(crate) fn parse_from(raw_args: &[OsString]) -> Result<MoonxInvocation, clap::Error> {
    let cli = MoonxCli::try_parse_from(raw_args)?;
    let MoonxPackage::External(package_and_args) = cli.package;
    let (package, args) = package_and_args
        .split_first()
        .expect("external subcommand always contains its name");
    // External subcommands preserve `--`; moonx treats one leading occurrence as a separator.
    let args = match args {
        [separator, args @ ..] if separator == "--" => args,
        _ => args,
    };
    let inherited_policy = std::env::var_os(moonutil::constants::MOONRUN_INHERITED_POLICY);
    // Moonx is only an intermediary. Remove the reserved value before
    // registry/cache work can start subprocesses; the prepared moonrun command
    // receives it explicitly below.
    if inherited_policy.is_some() {
        unsafe {
            std::env::remove_var(moonutil::constants::MOONRUN_INHERITED_POLICY);
        }
    }
    Ok(MoonxInvocation {
        target: cli.target,
        experimental_policy: cli.experimental_policy,
        verbose: cli.verbose,
        inherited_policy,
        package: package.clone(),
        args: args.to_vec(),
    })
}

pub(crate) fn prepare(
    invocation: MoonxInvocation,
    user_log: &UserLog,
) -> anyhow::Result<std::process::Command> {
    let quiet = !invocation.verbose;
    let target = match (invocation.target, invocation.inherited_policy) {
        (MoonxTarget::Native, Some(_)) => {
            anyhow::bail!("a sandboxed moonx invocation cannot use --target native")
        }
        (MoonxTarget::Wasm, inherited_policy) => RegistryRunTarget::Wasm {
            // The parent snapshot is authoritative. A child policy option may
            // not replace it, but remains valid for ordinary top-level moonx.
            experimental_policy: inherited_policy
                .is_none()
                .then_some(invocation.experimental_policy)
                .flatten(),
            inherited_policy,
        },
        (MoonxTarget::Native, None) if invocation.experimental_policy.is_some() => {
            anyhow::bail!("--experimental-policy is only valid with `--target wasm`")
        }
        (MoonxTarget::Native, None) => RegistryRunTarget::Native,
    };
    registry_runner::prepare(
        invocation.package,
        target,
        invocation.args,
        quiet,
        invocation.verbose,
        user_log,
    )
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
    fn requires_package() {
        let error = MoonxCli::try_parse_from(["moonx"]).unwrap_err();
        assert_eq!(
            error.kind(),
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn forwards_help_and_version_flags_after_package() {
        for flag in ["-h", "--help", "-V", "--version"] {
            let cli = MoonxCli::try_parse_from(["moonx", "user/module", flag]).unwrap();
            let MoonxPackage::External(package_and_args) = cli.package;
            assert_eq!(package_and_args, ["user/module", flag]);
        }
    }

    #[test]
    fn inherited_policy_rejects_native_execution_before_registry_work() {
        let error = prepare(
            MoonxInvocation {
                target: MoonxTarget::Native,
                experimental_policy: None,
                verbose: false,
                inherited_policy: Some(OsString::from("opaque-token")),
                package: "user/module".to_owned(),
                args: Vec::new(),
            },
            &UserLog::new(log::LevelFilter::Warn),
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "a sandboxed moonx invocation cannot use --target native"
        );
    }
}
