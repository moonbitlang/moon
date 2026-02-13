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

use std::path::Path;

fn stack_trace_line_number_regex() -> regex::Regex {
    regex::Regex::new(r"(?<redacted>:[0-9]+)(?:[ \t]+(?:at|by)|\n|$)")
        .expect("valid stack trace line number regex")
}

pub fn redaction_regex(pattern: &str) -> regex::Regex {
    regex::Regex::new(pattern).expect("valid redaction regex")
}

pub fn stack_trace_redactions(_src_dir: &Path) -> snapbox::Redactions {
    let mut redactions = snapbox::Redactions::new();
    redactions
        .insert("[LINE_NUMBER]", stack_trace_line_number_regex())
        .expect("valid stack trace line number redaction");
    redactions
        .insert(
            "[CORE_PATH]",
            regex::Regex::new(
                r"(?<redacted>(?:\$MOON_HOME|(?:[A-Za-z]:)?/[^ \t\r\n]*\.moon)/lib/core)",
            )
            .expect("valid moon core path regex"),
        )
        .expect("valid moon core path redaction");
    redactions
}
