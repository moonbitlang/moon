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

use std::io::Write;

use log::LevelFilter;

use crate::user_log::UserLog;

/// Owns the two MoonBuild-authored communication channels for one command.
///
/// Command results are fallible writes to stdout. Filtered user logs are
/// emitted through [`UserLog`] on stderr. Child-process passthrough, progress
/// displays, and tracing have separate output policies and do not use this
/// interface.
#[derive(Debug)]
pub struct CommandOutput {
    user_log: UserLog,
    quiet: bool,
}

impl CommandOutput {
    pub fn new(user_log_level: LevelFilter, quiet: bool) -> Self {
        Self {
            user_log: UserLog::new(user_log_level),
            quiet,
        }
    }

    pub fn user_log(&self) -> &UserLog {
        &self.user_log
    }

    /// Render one logical command result while holding the stdout lock.
    pub fn write_result<T, E>(
        &self,
        render: impl FnOnce(&mut dyn Write) -> Result<T, E>,
    ) -> Result<T, E> {
        let mut stdout = anstream::stdout().lock();
        render(&mut stdout)
    }

    /// Render an informational command status unless quiet output was requested.
    pub fn write_status<E>(
        &self,
        render: impl FnOnce(&mut dyn Write) -> Result<(), E>,
    ) -> Result<(), E> {
        if self.quiet {
            return Ok(());
        }
        self.write_result(render)
    }
}
