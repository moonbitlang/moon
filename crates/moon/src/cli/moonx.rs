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

use std::{ffi::OsString, path::Path, path::PathBuf};

use clap::Parser;
use mooncake::registry::RegistryClient;
use moonutil::cli_support::MoonxCli;
use moonutil::user_log::UserLog;

use super::registry_runner;

#[derive(Debug)]
pub(crate) struct MoonxInvocation {
    pub(crate) handoff: Option<moonutil::run_handoff::RunHandoff>,
    pub(crate) experimental_policy: Option<PathBuf>,
    pub(crate) virtual_process_tree: bool,
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
    let (input, args) = cli.input_and_args();
    let input = input.to_owned();
    let args = args.to_vec();
    Ok(MoonxInvocation {
        handoff: None,
        experimental_policy: cli.experimental_policy,
        virtual_process_tree: cli.virtual_process_tree,
        verbose: cli.verbose,
        input,
        args,
    })
}

pub(crate) fn prepare(
    invocation: MoonxInvocation,
    user_log: &UserLog,
) -> anyhow::Result<super::process::ProcessAction> {
    let MoonxInvocation {
        handoff,
        experimental_policy,
        virtual_process_tree,
        verbose,
        input,
        args,
    } = invocation;

    let policy_relay = moonutil::policy_transport::PolicyTransfer::take_from_env()?
        .map(moonutil::policy_transport::PolicyTransfer::into_relay);
    let standalone = if is_mbtx_input(&input) {
        let dirs = moonutil::project::SourceTargetDirs {
            cwd: None,
            target_dir: None,
        }
        .single_file_package_dirs(&input)?;
        Some(super::run::build_standalone_wasm(dirs, false, verbose)?)
    } else {
        None
    };
    let wasm_path = match &standalone {
        Some(built) => built.executable.clone(),
        None => RegistryClient::configured(user_log).acquire_executable_wasm(&input, user_log)?,
    };
    let (policy, policy_source_dir) = super::run::effective_moonrun_policy(
        experimental_policy.as_deref(),
        standalone
            .as_ref()
            .and_then(|built| built.embedded_mbtx_policy.as_ref()),
    );

    if let Some(handoff) = handoff {
        handoff.publish(
            &wasm_path,
            moonutil::run_handoff::ExecutionKind::Wasm,
            &args,
            policy,
            policy_source_dir,
        )?;
        return Ok(super::process::ProcessAction::Exit(0));
    }

    registry_runner::prepare_artifact(
        &wasm_path,
        policy,
        policy_source_dir,
        policy_relay,
        virtual_process_tree,
        &args,
        user_log,
    )
}

fn is_mbtx_input(input: &str) -> bool {
    Path::new(input)
        .extension()
        .is_some_and(|extension| extension == "mbtx")
}

#[cfg(test)]
mod tests {
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
    fn recognizes_standalone_mbtx_inputs() {
        assert!(is_mbtx_input("script.mbtx"));
        assert!(is_mbtx_input("path/to/script.mbtx"));
        assert!(!is_mbtx_input("user/module/package"));
        assert!(!is_mbtx_input("script.mbt"));
    }
}
