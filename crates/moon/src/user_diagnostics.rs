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

use anstyle::{AnsiColor, Style};
use moonutil::{
    cli_support::UniversalFlags,
    user_log::{UserLog, user_log_level},
    user_warning::{UserMessageLevel, UserWarning},
};

const INFO_STYLE: Style = AnsiColor::Cyan.on_default().bold();
const HINT_STYLE: Style = AnsiColor::Green.on_default().bold();

/// Compatibility adapter for call sites that still use diagnostic-style labels.
///
/// Filtering and stderr ownership belong to `UserLog`; this type only preserves
/// the legacy `Info:` and `Hint:` presentation until those callers migrate.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UserDiagnostics {
    user_log: UserLog,
}

impl UserDiagnostics {
    pub(crate) fn from_flags(flags: &UniversalFlags) -> Self {
        Self {
            user_log: UserLog::new(flags.user_log_level()),
        }
    }

    pub(crate) fn from_user_log(user_log: &UserLog) -> Self {
        Self {
            user_log: *user_log,
        }
    }

    pub(crate) fn new(verbose: bool, quiet: bool) -> Self {
        Self {
            user_log: UserLog::new(user_log_level(verbose, quiet)),
        }
    }

    pub(crate) fn error(self, message: impl Display) {
        self.user_log.error(message);
    }

    pub(crate) fn warn(self, message: impl Display) {
        self.user_log.warn(message);
    }

    pub(crate) fn info(self, message: impl Display) {
        self.user_log
            .info(format_args!("{INFO_STYLE}Info{INFO_STYLE:#}: {message}"));
    }

    pub(crate) fn user_message(self, message: &UserWarning) {
        match message.level() {
            UserMessageLevel::Warn => self.warn(message),
            UserMessageLevel::Info => self.info(message),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn hint(self, message: impl Display) {
        self.user_log
            .info(format_args!("{HINT_STYLE}Hint{HINT_STYLE:#}: {message}"));
    }
}

impl Default for UserDiagnostics {
    fn default() -> Self {
        Self::new(false, false)
    }
}
