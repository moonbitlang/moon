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
            command
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
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
    use std::io::{Read, Write};

    use log::LevelFilter;

    use super::{ChildOutputMode, run_managed_child};
    use crate::user_log::{UserLog, UserLogEntryLevel};

    const EMIT_HELPER_ENV: &str = "MOON_CHILD_OUTPUT_EMIT_HELPER";
    const INHERIT_HELPER_ENV: &str = "MOON_CHILD_OUTPUT_INHERIT_HELPER";
    const CAPTURE_STDIN_HELPER_ENV: &str = "MOON_CHILD_OUTPUT_CAPTURE_STDIN_HELPER";

    fn emitting_command(success: bool) -> std::process::Command {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "child_process::tests::emit_helper",
                "--nocapture",
            ])
            .env(EMIT_HELPER_ENV, success.to_string());
        command
    }

    #[test]
    fn emit_helper() -> Result<(), &'static str> {
        let Ok(success) = std::env::var(EMIT_HELPER_ENV) else {
            return Ok(());
        };

        print!("CHILD_STDOUT");
        eprint!("CHILD_STDERR");
        std::io::stdout().flush().unwrap();
        std::io::stderr().flush().unwrap();
        success
            .parse::<bool>()
            .unwrap()
            .then_some(())
            .ok_or("requested failure")
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
            assert!(
                entries[0]
                    .message
                    .starts_with("test child wrote to stdout:\n")
            );
            assert!(entries[0].message.contains("CHILD_STDOUT"));
            assert!(
                entries[1]
                    .message
                    .starts_with("test child wrote to stderr:\n")
            );
            assert!(entries[1].message.contains("CHILD_STDERR"));
        }
    }

    #[test]
    fn captured_output_closes_child_stdin() {
        let mut input = tempfile::NamedTempFile::new().unwrap();
        input.write_all(b"PARENT_STDIN").unwrap();

        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "child_process::tests::capture_stdin_helper",
                "--nocapture",
            ])
            .env(CAPTURE_STDIN_HELPER_ENV, "1")
            .stdin(std::fs::File::open(input.path()).unwrap());
        let (user_log, capture) = UserLog::captured(LevelFilter::Warn);

        let status = run_managed_child(
            &mut command,
            ChildOutputMode::Capture,
            &user_log,
            "test child",
        )
        .unwrap();

        assert!(status.success());
        let entries = capture.take();
        assert!(
            entries
                .iter()
                .any(|entry| entry.message.contains("STDIN_EOF")),
            "captured child inherited parent input: {entries:#?}"
        );
        assert!(
            entries
                .iter()
                .all(|entry| !entry.message.contains("PARENT_STDIN")),
            "captured child consumed parent input: {entries:#?}"
        );
    }

    #[test]
    fn capture_stdin_helper() {
        if std::env::var_os(CAPTURE_STDIN_HELPER_ENV).is_none() {
            return;
        }

        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).unwrap();
        if input.is_empty() {
            print!("STDIN_EOF");
        } else {
            print!("STDIN:{input}");
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
