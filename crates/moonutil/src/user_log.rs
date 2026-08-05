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

use std::{
    collections::HashSet,
    fmt::Display,
    io::Write,
    sync::{Arc, Mutex},
};

use anstyle::{AnsiColor, Style};
use log::LevelFilter;

const ERROR_STYLE: Style = AnsiColor::Red.on_default().bold();
const WARNING_STYLE: Style = AnsiColor::Yellow.on_default().bold();
const DEBUG_STYLE: Style = AnsiColor::BrightBlack.on_default().bold();

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UserLogEntryLevel {
    Error,
    Warning,
    Info,
    Debug,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UserLogEntry {
    pub level: UserLogEntryLevel,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct UserLogCapture {
    entries: Arc<Mutex<Vec<UserLogEntry>>>,
}

impl UserLogCapture {
    pub fn take(&self) -> Vec<UserLogEntry> {
        std::mem::take(&mut *self.entries.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

#[derive(Debug, Clone)]
enum UserLogDestination {
    Stderr,
    Capture(UserLogCapture),
}

#[derive(Debug, Clone)]
pub struct UserLog {
    level: LevelFilter,
    destination: UserLogDestination,
    emitted_once: Arc<Mutex<HashSet<String>>>,
}

/// Maps legacy CLI verbosity flags to the shared user-log level.
// FIXME: Remove this compatibility bridge once callers no longer receive raw
// `verbose` and `quiet` booleans.
pub fn user_log_level(verbose: bool, quiet: bool) -> LevelFilter {
    if quiet {
        LevelFilter::Error
    } else if verbose {
        LevelFilter::Info
    } else {
        LevelFilter::Warn
    }
}

impl UserLog {
    pub fn new(level: LevelFilter) -> Self {
        Self {
            level,
            destination: UserLogDestination::Stderr,
            emitted_once: Default::default(),
        }
    }

    pub fn captured(level: LevelFilter) -> (Self, UserLogCapture) {
        let capture = UserLogCapture::default();
        (
            Self {
                level,
                destination: UserLogDestination::Capture(capture.clone()),
                emitted_once: Default::default(),
            },
            capture,
        )
    }

    pub fn with_level(&self, level: LevelFilter) -> Self {
        Self {
            level,
            destination: self.destination.clone(),
            emitted_once: Arc::clone(&self.emitted_once),
        }
    }

    pub fn is_enabled(&self, level: log::Level) -> bool {
        self.level >= level.to_level_filter()
    }

    /// Whether user-facing output is being collected for a command result
    /// instead of rendered directly to stderr.
    pub fn is_captured(&self) -> bool {
        matches!(&self.destination, UserLogDestination::Capture(_))
    }

    pub fn error(&self, message: impl Display) {
        match &self.destination {
            UserLogDestination::Stderr => {
                let mut stderr = anstream::stderr().lock();
                self.error_to(&mut stderr, message);
            }
            UserLogDestination::Capture(capture) if self.level >= LevelFilter::Error => capture
                .entries
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(UserLogEntry {
                    level: UserLogEntryLevel::Error,
                    message: message.to_string(),
                }),
            UserLogDestination::Capture(_) => {}
        }
    }

    pub fn warn(&self, message: impl Display) {
        match &self.destination {
            UserLogDestination::Stderr => {
                let mut stderr = anstream::stderr().lock();
                self.warn_to(&mut stderr, message);
            }
            UserLogDestination::Capture(capture) if self.level >= LevelFilter::Warn => capture
                .entries
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(UserLogEntry {
                    level: UserLogEntryLevel::Warning,
                    message: message.to_string(),
                }),
            UserLogDestination::Capture(_) => {}
        }
    }

    pub fn warn_once(&self, message: impl Display) {
        if self.level < LevelFilter::Warn {
            return;
        }
        let message = message.to_string();
        if self
            .emitted_once
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(message.clone())
        {
            self.warn(message);
        }
    }

    pub fn info(&self, message: impl Display) {
        match &self.destination {
            UserLogDestination::Stderr => {
                let mut stderr = anstream::stderr().lock();
                self.info_to(&mut stderr, message);
            }
            UserLogDestination::Capture(capture) if self.level >= LevelFilter::Info => capture
                .entries
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(UserLogEntry {
                    level: UserLogEntryLevel::Info,
                    message: message.to_string(),
                }),
            UserLogDestination::Capture(_) => {}
        }
    }

    /// Emit a normal-verbosity informational status line.
    ///
    /// Some long-running operations historically showed progress at the
    /// default verbosity even though the message is informational. Keep that
    /// visibility without misclassifying the event as a warning.
    pub fn status(&self, message: impl Display) {
        match &self.destination {
            UserLogDestination::Stderr => {
                if self.level >= LevelFilter::Warn {
                    let _ = writeln!(anstream::stderr().lock(), "{message}");
                }
            }
            UserLogDestination::Capture(capture) if self.level >= LevelFilter::Warn => capture
                .entries
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(UserLogEntry {
                    level: UserLogEntryLevel::Info,
                    message: message.to_string(),
                }),
            UserLogDestination::Capture(_) => {}
        }
    }

    pub fn debug(&self, message: impl Display) {
        match &self.destination {
            UserLogDestination::Stderr => {
                let mut stderr = anstream::stderr().lock();
                self.debug_to(&mut stderr, message);
            }
            UserLogDestination::Capture(capture) if self.level >= LevelFilter::Debug => capture
                .entries
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(UserLogEntry {
                    level: UserLogEntryLevel::Debug,
                    message: message.to_string(),
                }),
            UserLogDestination::Capture(_) => {}
        }
    }

    fn error_to(&self, writer: &mut impl Write, message: impl Display) {
        if self.level >= LevelFilter::Error {
            let _ = writeln!(writer, "{ERROR_STYLE}Error{ERROR_STYLE:#}: {message}");
        }
    }

    fn warn_to(&self, writer: &mut impl Write, message: impl Display) {
        if self.level >= LevelFilter::Warn {
            let _ = writeln!(writer, "{WARNING_STYLE}Warning{WARNING_STYLE:#}: {message}");
        }
    }

    fn info_to(&self, writer: &mut impl Write, message: impl Display) {
        if self.level >= LevelFilter::Info {
            let _ = writeln!(writer, "{message}");
        }
    }

    fn debug_to(&self, writer: &mut impl Write, message: impl Display) {
        if self.level >= LevelFilter::Debug {
            let _ = writeln!(writer, "{DEBUG_STYLE}Debug{DEBUG_STYLE:#}: {message}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use anstream::{AutoStream, ColorChoice};
    use log::LevelFilter;

    use super::{UserLog, UserLogEntryLevel};

    #[test]
    fn captured_log_preserves_filtered_levels_and_order() {
        let (output, capture) = UserLog::captured(LevelFilter::Info);

        output.info("starting");
        output.warn("be careful");
        output.debug("hidden");
        output.error("failed");

        let entries = capture.take();
        assert_eq!(entries.len(), 3);
        assert!(matches!(entries[0].level, UserLogEntryLevel::Info));
        assert_eq!(entries[0].message, "starting");
        assert!(matches!(entries[1].level, UserLogEntryLevel::Warning));
        assert_eq!(entries[1].message, "be careful");
        assert!(matches!(entries[2].level, UserLogEntryLevel::Error));
        assert_eq!(entries[2].message, "failed");
    }

    #[test]
    fn captured_status_is_info_visible_at_normal_verbosity() {
        let (output, capture) = UserLog::captured(LevelFilter::Warn);

        output.status("Downloading example/pkg@1.0.0");

        let entries = capture.take();
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].level, UserLogEntryLevel::Info));
        assert_eq!(entries[0].message, "Downloading example/pkg@1.0.0");

        let (quiet, capture) = UserLog::captured(LevelFilter::Error);
        quiet.status("hidden");
        assert!(capture.take().is_empty());
    }

    #[test]
    fn cloned_logs_share_once_warnings_without_quiet_consuming_them() {
        let (output, capture) = UserLog::captured(LevelFilter::Warn);

        output
            .with_level(LevelFilter::Error)
            .warn_once("deprecated option");
        output.clone().warn_once("deprecated option");
        output.warn_once("deprecated option");

        let entries = capture.take();
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].level, UserLogEntryLevel::Warning));
        assert_eq!(entries[0].message, "deprecated option");
    }

    #[test]
    fn error_level_renders_error_and_suppresses_other_messages() {
        let output = UserLog::new(LevelFilter::Error);
        let mut writer = AutoStream::new(Vec::new(), ColorChoice::Never);

        output.error_to(&mut writer, "failed");
        output.warn_to(&mut writer, "be careful");
        output.info_to(&mut writer, "more context");
        output.debug_to(&mut writer, "internal context");

        assert_eq!(writer.into_inner(), b"Error: failed\n");
    }

    #[test]
    fn info_level_renders_error_warn_and_bare_info_but_not_debug() {
        let output = UserLog::new(LevelFilter::Info);
        let mut writer = AutoStream::new(Vec::new(), ColorChoice::Never);

        output.error_to(&mut writer, "failed");
        output.warn_to(&mut writer, "be careful");
        output.info_to(&mut writer, "more context");
        output.debug_to(&mut writer, "internal context");

        assert_eq!(
            writer.into_inner(),
            b"Error: failed\nWarning: be careful\nmore context\n"
        );
    }

    #[test]
    fn warn_level_renders_warn_but_not_info() {
        let output = UserLog::new(LevelFilter::Warn);
        let mut writer = AutoStream::new(Vec::new(), ColorChoice::Never);

        output.warn_to(&mut writer, "be careful");
        output.info_to(&mut writer, "more context");

        assert_eq!(writer.into_inner(), b"Warning: be careful\n");
    }

    #[test]
    fn debug_level_renders_debug() {
        let output = UserLog::new(LevelFilter::Debug);
        let mut writer = AutoStream::new(Vec::new(), ColorChoice::Never);

        output.debug_to(&mut writer, "internal context");

        assert_eq!(writer.into_inner(), b"Debug: internal context\n");
    }

    #[test]
    fn destination_writer_controls_color_output() {
        let output = UserLog::new(LevelFilter::Error);
        let mut colored = AutoStream::new(Vec::new(), ColorChoice::AlwaysAnsi);

        output.error_to(&mut colored, "failed");

        let colored = colored.into_inner();
        assert!(
            colored.starts_with(b"\x1b["),
            "output was not colored: {colored:?}"
        );

        let mut plain = AutoStream::new(Vec::new(), ColorChoice::Never);
        output.error_to(&mut plain, "failed");

        assert_eq!(plain.into_inner(), b"Error: failed\n");
    }

    #[test]
    fn write_errors_are_best_effort() {
        struct FailingWriter {
            attempts: usize,
        }

        impl Write for FailingWriter {
            fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
                self.attempts += 1;
                Err(std::io::Error::other("write failed"))
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let output = UserLog::new(LevelFilter::Debug);
        let mut writer = FailingWriter { attempts: 0 };

        output.error_to(&mut writer, "failed");
        output.warn_to(&mut writer, "be careful");
        output.info_to(&mut writer, "more context");
        output.debug_to(&mut writer, "internal context");

        assert_eq!(writer.attempts, 4);
    }
}
