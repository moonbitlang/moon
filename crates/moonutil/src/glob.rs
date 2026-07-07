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

/// Glob pattern matching supporting `*` (any sequence), `?` (any single character),
/// and other glob patterns. Uses the `globset` crate for robust matching.
pub enum GlobPatternMatcher<'a> {
    Compiled(globset::GlobMatcher),
    Literal(&'a str),
}

impl<'a> GlobPatternMatcher<'a> {
    pub fn new(pattern: &'a str) -> Self {
        use globset::GlobBuilder;
        let glob = GlobBuilder::new(pattern)
            .case_insensitive(false)
            .literal_separator(false)
            .build();
        match glob {
            Ok(glob) => Self::Compiled(glob.compile_matcher()),
            // If pattern is invalid, fall back to literal comparison
            Err(_) => Self::Literal(pattern),
        }
    }

    pub fn is_match(&self, text: &str) -> bool {
        match self {
            Self::Compiled(matcher) => matcher.is_match(text),
            Self::Literal(pattern) => pattern == &text,
        }
    }
}

/// Returns true if the text matches the pattern.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    GlobPatternMatcher::new(pattern).is_match(text)
}
