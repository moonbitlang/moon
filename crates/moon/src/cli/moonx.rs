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

use std::{ffi::OsString, path::Path, path::PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use mooncake::registry::RegistryClient;
use moonutil::user_log::UserLog;

use super::registry_runner::{self, RegistryRunTarget};

pub(crate) const NATIVE_TARGET_DEPRECATION_WARNING: &str =
    "`moonx --target native` is deprecated and scheduled for removal after 2026-09-14.";

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum MoonxTarget {
    #[default]
    Wasm,
    // TODO(2026-09-14): Remove the native moonx target and its registry
    // build/cache path after the two-week deprecation window.
    Native,
}

#[derive(Debug, Parser)]
#[command(
    name = "moonx",
    about = "Run a .mbtx file or a package from the Mooncakes registry",
    long_about = r#"Run a standalone .mbtx file or a package from the Mooncakes registry without installing it.

Accepted input forms:
  moonx script.mbtx
  moonx user/module/package
  moonx user/module/package@1.2.3
  moonx user/module/package@latest

Standalone .mbtx files always use the linear-memory Wasm backend. Pinned
package coordinates use the requested version directly. `@latest` refreshes the
registry index before resolving the latest version. Unpinned coordinates use
the latest version already known to the local registry index.

The native target is deprecated and scheduled for removal after 2026-09-14."#,
    override_usage = "moonx [OPTIONS] <MBTX|PACKAGE> [PROGRAM_ARGS]...",
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
    input: MoonxInput,
}

#[derive(Debug, Subcommand)]
enum MoonxInput {
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Debug)]
pub(crate) struct MoonxInvocation {
    pub(crate) target: MoonxTarget,
    pub(crate) experimental_policy: Option<PathBuf>,
    pub(crate) verbose: bool,
    input: String,
    args: Vec<String>,
}

pub(crate) fn is_moonx_invocation(raw_args: &[OsString]) -> bool {
    raw_args
        .first()
        .is_some_and(|arg| moonutil::constants::is_moonx_executable(arg))
}

pub(crate) fn parse_from(raw_args: &[OsString]) -> Result<MoonxInvocation, clap::Error> {
    let cli = MoonxCli::try_parse_from(raw_args)?;
    let MoonxInput::External(input_and_args) = cli.input;
    let (input, args) = input_and_args
        .split_first()
        .expect("external subcommand always contains its name");
    // External subcommands preserve `--`; moonx treats one leading occurrence as a separator.
    let args = match args {
        [separator, args @ ..] if separator == "--" => args,
        _ => args,
    };
    Ok(MoonxInvocation {
        target: cli.target,
        experimental_policy: cli.experimental_policy,
        verbose: cli.verbose,
        input: input.clone(),
        args: args.to_vec(),
    })
}

pub(crate) fn prepare(
    invocation: MoonxInvocation,
    user_log: &UserLog,
) -> anyhow::Result<super::process::ProcessAction> {
    let MoonxInvocation {
        target,
        experimental_policy,
        verbose,
        input,
        args,
    } = invocation;
    let quiet = !verbose;

    match target {
        MoonxTarget::Wasm => {
            let policy_relay = moonutil::policy_transport::PolicyTransfer::take_from_env()?
                .map(moonutil::policy_transport::PolicyTransfer::into_relay);
            let wasm_path = if is_mbtx_input(&input) {
                super::run::build_standalone_wasm(input, verbose)?
            } else {
                RegistryClient::configured().acquire_executable_wasm(&input, user_log)?
            };

            registry_runner::prepare_artifact(
                crate::run::ExecutionMode::MoonRun,
                &wasm_path,
                experimental_policy.as_deref(),
                policy_relay,
                &args,
                user_log,
            )
        }
        MoonxTarget::Native if is_mbtx_input(&input) => {
            anyhow::bail!("standalone `.mbtx` inputs only support `--target wasm`")
        }
        MoonxTarget::Native if experimental_policy.is_some() => {
            anyhow::bail!("--experimental-policy is only valid with `--target wasm`")
        }
        MoonxTarget::Native => {
            // Native execution has no descendant moonrun. Consume and close a
            // valid relay without letting an ambient malformed marker change
            // native behavior.
            moonutil::policy_transport::PolicyTransfer::discard_from_env();
            registry_runner::prepare(
                input,
                RegistryRunTarget::Native,
                args,
                quiet,
                verbose,
                user_log,
            )
        }
    }
}

fn is_mbtx_input(input: &str) -> bool {
    Path::new(input)
        .extension()
        .is_some_and(|extension| extension == "mbtx")
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
    fn requires_input() {
        let error = MoonxCli::try_parse_from(["moonx"]).unwrap_err();
        assert_eq!(
            error.kind(),
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn recognizes_standalone_mbtx_inputs() {
        assert!(is_mbtx_input("script.mbtx"));
        assert!(is_mbtx_input("path/to/script.mbtx"));
        assert!(!is_mbtx_input("user/module/package"));
        assert!(!is_mbtx_input("script.mbt"));
    }

    #[test]
    fn forwards_help_and_version_flags_after_input() {
        for flag in ["-h", "--help", "-V", "--version"] {
            let cli = MoonxCli::try_parse_from(["moonx", "user/module", flag]).unwrap();
            let MoonxInput::External(input_and_args) = cli.input;
            assert_eq!(input_and_args, ["user/module", flag]);
        }
    }
}
