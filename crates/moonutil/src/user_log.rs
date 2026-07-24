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

use std::{fmt::Display, io::Write};

use anstyle::{AnsiColor, Style};
use log::LevelFilter;

const ERROR_STYLE: Style = AnsiColor::Red.on_default().bold();
const WARNING_STYLE: Style = AnsiColor::Yellow.on_default().bold();

#[derive(Debug, Clone, Copy)]
pub struct UserLog {
    level: LevelFilter,
}

/// Maps legacy CLI verbosity flags to the shared user-log level.
// FIXME: Remove this compatibility bridge once callers no longer receive raw
// `verbose` and `quiet` booleans.
pub fn user_log_level(verbose: bool, quiet: bool) -> LevelFilter {
    if quiet {
        LevelFilter::Error
    } else if verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    }
}

impl UserLog {
    pub fn new(level: LevelFilter) -> Self {
        Self { level }
    }

    pub fn error(&self, message: impl Display) {
        let mut stderr = anstream::stderr().lock();
        self.error_to(&mut stderr, message);
    }

    pub fn warn(&self, message: impl Display) {
        let mut stderr = anstream::stderr().lock();
        self.warn_to(&mut stderr, message);
    }

    pub fn info(&self, message: impl Display) {
        let mut stderr = anstream::stderr().lock();
        self.info_to(&mut stderr, message);
    }

    pub fn debug(&self, message: impl Display) {
        let mut stderr = anstream::stderr().lock();
        self.debug_to(&mut stderr, message);
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
            let _ = writeln!(writer, "{message}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use anstream::{AutoStream, ColorChoice};
    use log::LevelFilter;

    use super::{UserLog, user_log_level};

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
    fn debug_level_renders_bare_debug() {
        let output = UserLog::new(LevelFilter::Debug);
        let mut writer = AutoStream::new(Vec::new(), ColorChoice::Never);

        output.debug_to(&mut writer, "internal context");

        assert_eq!(writer.into_inner(), b"internal context\n");
    }

    #[test]
    fn cli_verbosity_maps_to_user_log_detail() {
        assert_eq!(user_log_level(false, true), LevelFilter::Error);
        assert_eq!(user_log_level(false, false), LevelFilter::Info);
        assert_eq!(user_log_level(true, false), LevelFilter::Debug);
        assert_eq!(user_log_level(true, true), LevelFilter::Error);
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
