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

//! Compatibility execution for the legacy `scripts.postadd` hook.
//!
//! Source installation only warns. Execution belongs to `moon run-postadd`.

// TODO: Remove the compatibility command after remaining toolchain integration
// use cases are reported in project issues and migrated away from postadd.

use std::path::Path;

use anyhow::bail;
use moonutil::{child_process::ManagedChildRunner, manifest::MoonMod, user_log::UserLog};

pub fn warn_skipped(module: &MoonMod, user_log: &UserLog) {
    if module
        .scripts
        .as_ref()
        .is_some_and(|scripts| scripts.contains_key("postadd"))
    {
        user_log.warn(format!(
            "Skipped postadd for {}; use `moon run-postadd --help` to run it explicitly.",
            module.name
        ));
    }
}

/// Run the legacy hook declared by the materialized module at `module_dir`.
pub fn run(module_dir: &Path, postadd: &str, runner: &ManagedChildRunner) -> anyhow::Result<()> {
    let postadd = postadd.split(' ').collect::<Vec<_>>();
    let Some((command, args)) = postadd.split_first() else {
        return Ok(());
    };
    let mut process = std::process::Command::new(command);
    process.args(args).current_dir(module_dir);
    let status = runner.run(&mut process, "postadd script")?;
    if !status.success() {
        bail!(
            "failed to execute postadd script in {},\ncommand: {}",
            module_dir.display(),
            command
        );
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use log::LevelFilter;
    use moonutil::{
        child_process::{ChildOutputMode, ManagedChildRunner},
        user_log::{UserLog, UserLogEntryLevel},
    };

    use super::run;

    #[test]
    fn captured_output_is_routed_to_user_log() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let script = sandbox.path().join("postadd.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\necho POSTADD_STDOUT\necho POSTADD_STDERR >&2\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (user_log, capture) = UserLog::captured(LevelFilter::Warn);
        let child = ManagedChildRunner::new(ChildOutputMode::Capture, &user_log);

        run(sandbox.path(), script.to_str().unwrap(), &child).unwrap();

        let entries = capture.take();
        assert_eq!(entries.len(), 2);
        assert!(
            entries
                .iter()
                .all(|entry| matches!(entry.level, UserLogEntryLevel::Info))
        );
        assert!(entries[0].message.contains("POSTADD_STDOUT"));
        assert!(entries[1].message.contains("POSTADD_STDERR"));

        std::fs::write(
            &script,
            "#!/bin/sh\necho FAILED_STDOUT\necho FAILED_STDERR >&2\nexit 1\n",
        )
        .unwrap();
        let (user_log, capture) = UserLog::captured(LevelFilter::Warn);
        let child = ManagedChildRunner::new(ChildOutputMode::Capture, &user_log);

        let error = run(sandbox.path(), script.to_str().unwrap(), &child).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("failed to execute postadd script")
        );
        let entries = capture.take();
        assert_eq!(entries.len(), 2);
        assert!(
            entries
                .iter()
                .all(|entry| matches!(entry.level, UserLogEntryLevel::Error))
        );
        assert!(entries[0].message.contains("FAILED_STDOUT"));
        assert!(entries[1].message.contains("FAILED_STDERR"));
    }
}
