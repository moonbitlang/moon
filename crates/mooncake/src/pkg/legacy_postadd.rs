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

//! Compatibility execution for the legacy `scripts.postadd` hook.
//!
//! Registry source acquisition deliberately does not call this module. The
//! remaining command paths opt into the legacy behavior after source has been
//! materialized, so rejecting postadd in the future does not change the
//! Registry interface. This compatibility module preserves the existing child
//! output behavior; making that lifecycle explicit belongs to the separate
//! command-output migration.

use std::path::Path;

use anyhow::bail;
use moonutil::{manifest::read_module_desc_file_in_dir, user_log::UserLog};

pub fn run(dir: &Path, user_log: &UserLog) -> anyhow::Result<()> {
    if std::env::var_os("MOON_IGNORE_POSTADD").is_some() {
        return Ok(());
    }
    let module = read_module_desc_file_in_dir(dir)?;
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
    process.args(args).current_dir(dir);
    if user_log.is_captured() {
        let output = process.output()?;
        for (channel, content) in [
            ("stdout", output.stdout.as_slice()),
            ("stderr", output.stderr.as_slice()),
        ] {
            let content = String::from_utf8_lossy(content);
            let content = content.trim();
            if content.is_empty() {
                continue;
            }
            let message = format!("postadd script wrote to {channel}:\n{content}");
            if output.status.success() {
                user_log.status(message);
            } else {
                user_log.error(message);
            }
        }
        if !output.status.success() {
            bail!(
                "failed to execute postadd script in {},\ncommand: {}",
                dir.display(),
                command
            );
        }
    } else {
        let status = process
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()?;
        if !status.success() {
            bail!(
                "failed to execute postadd script in {},\ncommand: {}",
                dir.display(),
                command
            );
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use log::LevelFilter;
    use moonutil::user_log::{UserLog, UserLogEntryLevel};

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

        run(sandbox.path(), &user_log).unwrap();

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

        let error = run(sandbox.path(), &user_log).unwrap_err();

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
