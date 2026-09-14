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
//! Registry source acquisition deliberately does not call this module. The
//! remaining command paths opt into the legacy behavior after source has been
//! materialized, so rejecting postadd in the future does not change the
//! Registry interface.

use std::path::Path;

use anyhow::bail;
use moonutil::{child_process::ManagedChildRunner, manifest::read_module_desc_file_in_dir};

/// Run the legacy hook declared by the materialized module at `module_dir`.
pub fn run(module_dir: &Path, runner: &ManagedChildRunner) -> anyhow::Result<()> {
    if std::env::var_os("MOON_IGNORE_POSTADD").is_some() {
        return Ok(());
    }
    let module = read_module_desc_file_in_dir(module_dir)?;
    let Some(postadd) = module
        .scripts
        .as_ref()
        .and_then(|scripts| scripts.get("postadd"))
    else {
        return Ok(());
    };

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
        std::fs::write(
            sandbox.path().join("moon.mod.json"),
            format!(
                r#"{{
                    "name": "test/postadd",
                    "version": "0.1.0",
                    "scripts": {{ "postadd": "{}" }}
                }}"#,
                script.display()
            ),
        )
        .unwrap();
        let (user_log, capture) = UserLog::captured(LevelFilter::Warn);
        let child = ManagedChildRunner::new(ChildOutputMode::Capture, &user_log);

        run(sandbox.path(), &child).unwrap();

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

        let error = run(sandbox.path(), &child).unwrap_err();

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
