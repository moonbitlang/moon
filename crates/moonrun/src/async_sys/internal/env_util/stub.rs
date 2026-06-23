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

use crate::async_sys::ported_fns;

ported_fns! {
    #[ported(
        source = "src/internal/env_util/stub.c",
        original = "moonbitlang_async_getpid"
    )]
    pub(crate) fn get_pid() -> u32 {
        std::process::id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_pid_matches_current_process() {
        assert_eq!(get_pid(), std::process::id());
    }
}
