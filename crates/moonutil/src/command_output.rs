// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::io::Write;

use log::LevelFilter;

use crate::user_log::{UserLog, UserLogCapture};

/// Owns the two MoonBuild-authored communication channels for one command.
///
/// Command results are fallible writes to stdout. Filtered user logs are
/// emitted through [`UserLog`] on stderr. Child-process passthrough, progress
/// displays, and tracing have separate output policies and do not use this
/// interface.
#[derive(Debug)]
pub struct CommandOutput {
    user_log: UserLog,
}

impl CommandOutput {
    pub fn new(user_log_level: LevelFilter) -> Self {
        Self {
            user_log: UserLog::new(user_log_level),
        }
    }

    /// Construct an output facade whose User Logs are captured for later
    /// inclusion in a structured Command Result.
    pub fn captured(user_log_level: LevelFilter) -> (Self, UserLogCapture) {
        let (user_log, capture) = UserLog::captured(user_log_level);
        (Self { user_log }, capture)
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
}
