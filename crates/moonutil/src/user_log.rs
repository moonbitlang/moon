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

use colored::Colorize;
use log::LevelFilter;

#[derive(Debug)]
pub struct UserLog {
    level: LevelFilter,
}

impl UserLog {
    pub fn new(level: LevelFilter) -> Self {
        Self { level }
    }

    pub fn error(&self, message: impl Display) {
        let mut stderr = std::io::stderr().lock();
        self.error_to(&mut stderr, message);
    }

    pub fn warn(&self, message: impl Display) {
        let mut stderr = std::io::stderr().lock();
        self.warn_to(&mut stderr, message);
    }

    pub fn info(&self, message: impl Display) {
        let mut stderr = std::io::stderr().lock();
        self.info_to(&mut stderr, message);
    }

    pub fn debug(&self, message: impl Display) {
        let mut stderr = std::io::stderr().lock();
        self.debug_to(&mut stderr, message);
    }

    fn error_to(&self, writer: &mut impl Write, message: impl Display) {
        if self.level >= LevelFilter::Error {
            let _ = writeln!(writer, "{}: {}", "Error".red().bold(), message);
        }
    }

    fn warn_to(&self, writer: &mut impl Write, message: impl Display) {
        if self.level >= LevelFilter::Warn {
            let _ = writeln!(writer, "{}: {}", "Warning".yellow().bold(), message);
        }
    }

    fn info_to(&self, writer: &mut impl Write, message: impl Display) {
        if self.level >= LevelFilter::Info {
            let _ = writeln!(writer, "{message}");
        }
    }

    fn debug_to(&self, writer: &mut impl Write, message: impl Display) {
        if self.level >= LevelFilter::Debug {
            let _ = writeln!(writer, "{}: {}", "Debug".bright_black().bold(), message);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use log::LevelFilter;

    use super::UserLog;

    #[test]
    fn error_level_renders_error_and_suppresses_other_messages() {
        let output = UserLog::new(LevelFilter::Error);
        let mut writer = Vec::new();

        output.error_to(&mut writer, "failed");
        output.warn_to(&mut writer, "be careful");
        output.info_to(&mut writer, "more context");
        output.debug_to(&mut writer, "internal context");

        assert_eq!(writer, b"Error: failed\n");
    }

    #[test]
    fn info_level_renders_error_warn_and_bare_info_but_not_debug() {
        let output = UserLog::new(LevelFilter::Info);
        let mut writer = Vec::new();

        output.error_to(&mut writer, "failed");
        output.warn_to(&mut writer, "be careful");
        output.info_to(&mut writer, "more context");
        output.debug_to(&mut writer, "internal context");

        assert_eq!(
            writer,
            b"Error: failed\nWarning: be careful\nmore context\n"
        );
    }

    #[test]
    fn warn_level_renders_warn_but_not_info() {
        let output = UserLog::new(LevelFilter::Warn);
        let mut writer = Vec::new();

        output.warn_to(&mut writer, "be careful");
        output.info_to(&mut writer, "more context");

        assert_eq!(writer, b"Warning: be careful\n");
    }

    #[test]
    fn debug_level_renders_debug() {
        let output = UserLog::new(LevelFilter::Debug);
        let mut writer = Vec::new();

        output.debug_to(&mut writer, "internal context");

        assert_eq!(writer, b"Debug: internal context\n");
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
