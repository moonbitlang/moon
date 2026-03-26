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

use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserMessageLevel {
    Warn,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserWarning {
    level: UserMessageLevel,
    message: String,
}

impl UserWarning {
    pub fn new(message: impl Into<String>) -> Self {
        Self::warn(message)
    }

    pub fn warn(message: impl Into<String>) -> Self {
        Self {
            level: UserMessageLevel::Warn,
            message: message.into(),
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: UserMessageLevel::Info,
            message: message.into(),
        }
    }

    pub fn level(&self) -> UserMessageLevel {
        self.level
    }
}

impl Display for UserWarning {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
