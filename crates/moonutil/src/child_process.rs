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

use std::process::{Command, ExitStatus, Stdio};

use crate::user_log::UserLog;

/// Output policy for Moon-managed child processes.
///
/// This does not apply to user programs, whose output remains Process
/// Passthrough. Capture is reserved for command modes that must incorporate
/// all Moon-visible output into one structured Command Result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChildOutputMode {
    #[default]
    Inherit,
    Capture,
}

/// Run a Moon-managed child and classify captured output as User Logs.
///
/// Inherited output retains its original byte stream and channel. Captured
/// output becomes informational status on success and an error on failure.
pub fn run_managed_child(
    command: &mut Command,
    output_mode: ChildOutputMode,
    user_log: &UserLog,
    subject: &str,
) -> std::io::Result<ExitStatus> {
    match output_mode {
        ChildOutputMode::Inherit => {
            command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        }
        ChildOutputMode::Capture => {
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
        }
    }

    let mut child = command.spawn()?;
    match output_mode {
        ChildOutputMode::Inherit => child.wait(),
        ChildOutputMode::Capture => {
            let output = child.wait_with_output()?;
            for (channel, content) in [
                ("stdout", output.stdout.as_slice()),
                ("stderr", output.stderr.as_slice()),
            ] {
                let content = String::from_utf8_lossy(content);
                let content = content.trim();
                if content.is_empty() {
                    continue;
                }
                let message = format!("{subject} wrote to {channel}:\n{content}");
                if output.status.success() {
                    user_log.status(message);
                } else {
                    user_log.error(message);
                }
            }
            Ok(output.status)
        }
    }
}

#[cfg(test)]
mod tests {
    use log::LevelFilter;

    use super::{ChildOutputMode, run_managed_child};
    use crate::user_log::{UserLog, UserLogEntryLevel};

    const INHERIT_HELPER_ENV: &str = "MOON_CHILD_OUTPUT_INHERIT_HELPER";

    fn emitting_command(success: bool) -> std::process::Command {
        #[cfg(unix)]
        {
            let mut command = std::process::Command::new("/bin/sh");
            command.args([
                "-c",
                if success {
                    "printf CHILD_STDOUT; printf CHILD_STDERR >&2"
                } else {
                    "printf CHILD_STDOUT; printf CHILD_STDERR >&2; exit 7"
                },
            ]);
            command
        }
        #[cfg(windows)]
        {
            let mut command = std::process::Command::new("cmd");
            command.args([
                "/C",
                if success {
                    "<nul set /p =CHILD_STDOUT & <nul set /p =CHILD_STDERR 1>&2"
                } else {
                    "<nul set /p =CHILD_STDOUT & <nul set /p =CHILD_STDERR 1>&2 & exit /b 7"
                },
            ]);
            command
        }
    }

    #[test]
    fn captured_output_uses_status_on_success_and_error_on_failure() {
        for success in [true, false] {
            let (user_log, capture) = UserLog::captured(LevelFilter::Warn);
            let status = run_managed_child(
                &mut emitting_command(success),
                ChildOutputMode::Capture,
                &user_log,
                "test child",
            )
            .unwrap();

            assert_eq!(status.success(), success);
            let entries = capture.take();
            assert_eq!(entries.len(), 2);
            assert!(entries.iter().all(|entry| if success {
                matches!(entry.level, UserLogEntryLevel::Info)
            } else {
                matches!(entry.level, UserLogEntryLevel::Error)
            }));
            assert!(entries[0].message.contains("stdout:\nCHILD_STDOUT"));
            assert!(entries[1].message.contains("stderr:\nCHILD_STDERR"));
        }
    }

    #[test]
    fn inherited_output_keeps_parent_channels_on_success_and_failure() {
        for success in [true, false] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "child_process::tests::inherit_helper",
                    "--nocapture",
                ])
                .env(INHERIT_HELPER_ENV, success.to_string())
                .output()
                .unwrap();

            assert!(output.status.success());
            assert!(String::from_utf8_lossy(&output.stdout).contains("CHILD_STDOUT"));
            assert!(String::from_utf8_lossy(&output.stderr).contains("CHILD_STDERR"));
        }
    }

    #[test]
    fn inherit_helper() {
        let Ok(success) = std::env::var(INHERIT_HELPER_ENV) else {
            return;
        };
        let success = success.parse().unwrap();
        let status = run_managed_child(
            &mut emitting_command(success),
            ChildOutputMode::Inherit,
            &UserLog::new(LevelFilter::Warn),
            "test child",
        )
        .unwrap();

        assert_eq!(status.success(), success);
    }
}
