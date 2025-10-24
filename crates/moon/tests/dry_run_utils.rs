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

//! Utilities to handle dry run outputs

use std::fmt::Debug;

/// Find a line with the given command and contains the filter strings
pub fn line_with<T: AsRef<str> + Debug>(
    input: impl AsRef<str>,
    command: impl AsRef<str>,
    filter: &[T],
) -> String {
    let lines = input.as_ref().lines();
    for line in lines {
        if line.contains(command.as_ref()) && filter.iter().all(|f| line.contains(f.as_ref())) {
            return line.to_string();
        }
    }
    panic!(
        "No line found with command: {} and filter {:?}",
        command.as_ref(),
        filter
    );
}

/// Return the shlex-split tokens of a command line from a dry-run output.
pub fn command_tokens<T: AsRef<str> + Debug>(
    input: impl AsRef<str>,
    command: impl AsRef<str>,
    filter: &[T],
) -> Vec<String> {
    let line = line_with(input, command, filter);
    shlex::split(&line).unwrap_or_default()
}
