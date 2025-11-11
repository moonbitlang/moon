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

/// Ensures the expected lines appear in order within the actual output, allowing
/// unrelated lines to exist between matches.
pub fn assert_lines_in_order(actual: impl AsRef<str>, expect: impl AsRef<str>) {
    let actual = actual.as_ref();
    let expect = expect.as_ref();

    let actual_lines: Vec<&str> = actual.trim().lines().collect();
    let expect_lines: Vec<&str> = expect.trim().lines().collect();

    let mut pos = 0;
    for expect_line in expect_lines {
        let start_pos = pos;
        let mut found = false;
        while pos < actual_lines.len() {
            if actual_lines[pos].trim() == expect_line.trim() {
                found = true;
                pos += 1;
                break;
            }
            pos += 1;
        }

        if !found {
            println!("Unable to find expected line: {:?}", expect_line.trim());
            println!("Search started from line {}:", start_pos + 1);
            for (off, line) in actual_lines[start_pos..].iter().enumerate() {
                println!("{:>3} | {}", start_pos + off + 1, line);
            }
            panic!("Expected line not found in order.");
        }
    }
}
