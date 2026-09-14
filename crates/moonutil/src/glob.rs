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
