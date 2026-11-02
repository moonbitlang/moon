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

use crate::rr_build;
use moonutil::user_log::UserLog;
use std::path::Path;

/// Prepare one already-acquired or already-built artifact for final execution.
pub(crate) fn prepare_artifact(
    artifact: &Path,
    experimental_policy: Option<&Path>,
    policy_source_dir: Option<&Path>,
    policy_relay: Option<moonutil::policy_transport::PolicyRelay>,
    args: &[String],
    user_log: &UserLog,
) -> anyhow::Result<super::process::ProcessAction> {
    let mut run_cmd = crate::run::command_for_with_moonrun_policy_source_dir(
        crate::run::ExecutionMode::MoonRun,
        artifact,
        None,
        experimental_policy,
        policy_source_dir,
    );
    run_cmd.args(args);

    if user_log.is_enabled(log::Level::Info) {
        user_log.info(rr_build::format_dry_run_command(&run_cmd, Path::new(".")));
    }

    // Inheritance is attached immediately before delegation. Keep any ambient
    // marker out of the prepared command in both inherited and ordinary runs.
    run_cmd.env_remove(moonutil::constants::MOONRUN_INHERITED_POLICY);

    Ok(match policy_relay {
        Some(relay) => super::process::ProcessAction::DelegateWithPolicyRelay(run_cmd, relay),
        None => super::process::ProcessAction::Delegate(run_cmd),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn ordinary_registry_runs_remove_an_ambient_inherited_policy() {
        let action = prepare_artifact(
            Path::new("artifact.wasm"),
            None,
            None,
            None,
            &[],
            &UserLog::new(log::LevelFilter::Warn),
        )
        .unwrap();
        let crate::cli::process::ProcessAction::Delegate(command) = action else {
            panic!("ordinary registry run unexpectedly carried an inherited policy")
        };
        assert!(command.get_envs().any(|(name, value)| {
            name == OsStr::new(moonutil::constants::MOONRUN_INHERITED_POLICY) && value.is_none()
        }));
    }
}
