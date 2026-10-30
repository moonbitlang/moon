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

//! Utilities to handle dry run outputs

use std::fmt::Debug;

/// Find a line with the given command and contains the filter strings
pub(crate) fn line_with<T: AsRef<str> + Debug>(
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

/// Ensures the expected lines appear in order within the actual output, allowing
/// unrelated lines to exist between matches.
pub(crate) fn assert_lines_in_order(actual: impl AsRef<str>, expect: impl AsRef<str>) {
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
