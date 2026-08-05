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

use std::path::Path;

use anyhow::bail;

use crate::{manifest::read_module_desc_file_in_dir, user_log::UserLog};

pub enum PrePostBuild {
    PreBuild,
}

impl PrePostBuild {
    pub fn name(&self) -> String {
        match self {
            PrePostBuild::PreBuild => "pre-build".into(),
        }
    }

    pub fn dbname(&self) -> String {
        format!("{}.db", self.name())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum IgnoredMoonScript {
    Prebuild,
    Postadd,
}

impl IgnoredMoonScript {
    pub fn env_var(self) -> &'static str {
        match self {
            IgnoredMoonScript::Prebuild => "MOON_IGNORE_PREBUILD",
            IgnoredMoonScript::Postadd => "MOON_IGNORE_POSTADD",
        }
    }
}

pub fn is_moon_script_ignored(script: IgnoredMoonScript) -> bool {
    std::env::var_os(script.env_var()).is_some()
}

pub fn execute_postadd_script(dir: &Path, user_log: &UserLog) -> anyhow::Result<()> {
    if is_moon_script_ignored(IgnoredMoonScript::Postadd) {
        return Ok(());
    }
    let m = read_module_desc_file_in_dir(dir)?;
    if let Some(scripts) = &m.scripts
        && scripts.contains_key("postadd")
    {
        let postadd = scripts
            .get("postadd")
            .unwrap()
            .split(' ')
            .collect::<Vec<_>>();
        if !postadd.is_empty() {
            let command = postadd[0];
            let args = &postadd[1..];
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
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use log::LevelFilter;

    use super::execute_postadd_script;
    use crate::user_log::{UserLog, UserLogEntryLevel};

    #[test]
    fn captured_postadd_output_is_routed_to_user_log() {
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

        execute_postadd_script(sandbox.path(), &user_log).unwrap();

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

        let error = execute_postadd_script(sandbox.path(), &user_log).unwrap_err();

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
