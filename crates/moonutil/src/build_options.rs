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

use std::{path::PathBuf, str::FromStr};

#[derive(Debug, Clone, Copy, Hash)]
pub struct TestIndexRange {
    pub start: u32,
    pub end: u32,
}

impl TestIndexRange {
    pub fn from_single(index: u32) -> Result<Self, TestIndexRangeParseError> {
        let end = index
            .checked_add(1)
            .ok_or(TestIndexRangeParseError::EndOverflow)?;
        Ok(Self { start: index, end })
    }

    pub fn contains(self, index: u32) -> bool {
        self.start <= index && index < self.end
    }

    pub fn as_range(self) -> std::ops::Range<u32> {
        self.start..self.end
    }
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum TestIndexRangeParseError {
    #[error("index is empty")]
    Empty,
    #[error("missing range start")]
    MissingStart,
    #[error("missing range end")]
    MissingEnd,
    #[error("invalid number `{0}`")]
    InvalidNumber(String),
    #[error("range end must be greater than start")]
    InvalidRange,
    #[error("range end overflows u32")]
    EndOverflow,
}

impl FromStr for TestIndexRange {
    type Err = TestIndexRangeParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let s = input.trim();
        if s.is_empty() {
            return Err(TestIndexRangeParseError::Empty);
        }

        if s.contains("..") {
            return Err(TestIndexRangeParseError::InvalidRange);
        }

        if let Some((start, end)) = s.split_once('-') {
            if end.contains('-') {
                return Err(TestIndexRangeParseError::InvalidRange);
            }
            let start = parse_index_bound(start, TestIndexRangeParseError::MissingStart)?;
            let end = parse_index_bound(end, TestIndexRangeParseError::MissingEnd)?;
            if start >= end {
                return Err(TestIndexRangeParseError::InvalidRange);
            }
            return Ok(Self { start, end });
        }

        let start = parse_index_bound(s, TestIndexRangeParseError::Empty)?;
        TestIndexRange::from_single(start)
    }
}

fn parse_index_bound(
    s: &str,
    empty_error: TestIndexRangeParseError,
) -> Result<u32, TestIndexRangeParseError> {
    if s.is_empty() {
        return Err(empty_error);
    }
    s.parse::<u32>()
        .map_err(|_| TestIndexRangeParseError::InvalidNumber(s.to_string()))
}

#[derive(serde::Serialize, Clone)]
pub struct TestArtifacts {
    pub artifacts_path: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test_filter_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum RunMode {
    Bench,
    Build,
    Check,
    Prove,
    Run,
    Test,
    Bundle,
    Format,
}

impl RunMode {
    pub fn to_dir_name(self) -> &'static str {
        match self {
            Self::Bench => "bench",
            Self::Build | Self::Run => "build",
            Self::Check => "check",
            Self::Prove => "prove",
            Self::Test => "test",
            Self::Bundle => "bundle",
            Self::Format => "format",
        }
    }
}
