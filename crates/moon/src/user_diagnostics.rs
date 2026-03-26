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

use std::fmt::Display;

use colored::{ColoredString, Colorize};
use moonutil::cli::UniversalFlags;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum UserThreshold {
    Warn,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UserMessageKind {
    Error,
    Warn,
    Info,
    #[allow(dead_code)]
    Hint,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct UserDiagnostics {
    threshold: UserThreshold,
    quiet: bool,
}

impl UserDiagnostics {
    pub(crate) fn from_flags(flags: &UniversalFlags) -> Self {
        Self::new(flags.verbose, flags.quiet)
    }

    pub(crate) fn new(verbose: bool, quiet: bool) -> Self {
        Self {
            threshold: if verbose {
                UserThreshold::Info
            } else {
                UserThreshold::Warn
            },
            quiet,
        }
    }

    pub(crate) fn error(self, message: impl Display) {
        self.emit(UserMessageKind::Error, message);
    }

    pub(crate) fn warn(self, message: impl Display) {
        self.emit(UserMessageKind::Warn, message);
    }

    pub(crate) fn info(self, message: impl Display) {
        self.emit(UserMessageKind::Info, message);
    }

    #[allow(dead_code)]
    pub(crate) fn hint(self, message: impl Display) {
        self.emit(UserMessageKind::Hint, message);
    }

    fn emit(self, kind: UserMessageKind, message: impl Display) {
        if self.enabled(kind) {
            eprintln!("{}: {}", kind.label(), message);
        }
    }

    fn enabled(self, kind: UserMessageKind) -> bool {
        match kind {
            UserMessageKind::Error | UserMessageKind::Warn => true,
            UserMessageKind::Info | UserMessageKind::Hint => {
                !self.quiet && self.threshold >= UserThreshold::Info
            }
        }
    }
}

impl Default for UserDiagnostics {
    fn default() -> Self {
        Self::new(false, false)
    }
}

impl UserMessageKind {
    fn label(self) -> ColoredString {
        match self {
            UserMessageKind::Error => "Error".red().bold(),
            UserMessageKind::Warn => "Warning".yellow().bold(),
            UserMessageKind::Info => "Info".cyan().bold(),
            UserMessageKind::Hint => "Hint".green().bold(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{UserDiagnostics, UserMessageKind};

    #[test]
    fn default_output_shows_warn_but_not_info() {
        let output = UserDiagnostics::new(false, false);
        assert!(output.enabled(UserMessageKind::Warn));
        assert!(output.enabled(UserMessageKind::Error));
        assert!(!output.enabled(UserMessageKind::Info));
        assert!(!output.enabled(UserMessageKind::Hint));
    }

    #[test]
    fn verbose_output_enables_info() {
        let output = UserDiagnostics::new(true, false);
        assert!(output.enabled(UserMessageKind::Info));
        assert!(output.enabled(UserMessageKind::Hint));
    }

    #[test]
    fn quiet_output_still_shows_warn() {
        let output = UserDiagnostics::new(true, true);
        assert!(output.enabled(UserMessageKind::Warn));
        assert!(output.enabled(UserMessageKind::Error));
        assert!(!output.enabled(UserMessageKind::Info));
        assert!(!output.enabled(UserMessageKind::Hint));
    }
}
