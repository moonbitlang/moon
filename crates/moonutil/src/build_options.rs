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

use std::{collections::HashSet, path::PathBuf, str::FromStr};

use crate::{
    target::{OutputFormat, TargetBackend},
    test_metadata::DiagnosticLevel,
};

#[derive(Debug, Clone)]
pub struct BuildPackageFlags {
    pub debug_flag: bool,
    pub strip_flag: bool,
    pub source_map: bool,
    pub enable_coverage: bool,
    // treat all warnings as errors
    pub deny_warn: bool,
    pub target_backend: TargetBackend,
    pub warn_list: Option<String>,
    pub enable_value_tracing: bool,
}

impl BuildPackageFlags {
    pub fn new() -> Self {
        Self {
            debug_flag: false,
            strip_flag: true,
            source_map: false,
            enable_coverage: false,
            deny_warn: false,
            target_backend: TargetBackend::default(),
            warn_list: None,
            enable_value_tracing: false,
        }
    }
}

impl Default for BuildPackageFlags {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LinkCoreFlags {
    pub debug_flag: bool,
    pub source_map: bool,
    pub output_format: OutputFormat,
    pub target_backend: TargetBackend,
}

impl LinkCoreFlags {
    pub fn new() -> Self {
        Self {
            debug_flag: false,
            source_map: false,
            output_format: OutputFormat::Wasm,
            target_backend: TargetBackend::default(),
        }
    }
}

impl Default for LinkCoreFlags {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct MoonbuildOpt {
    pub source_dir: PathBuf,
    pub raw_target_dir: PathBuf,
    pub target_dir: PathBuf,
    pub test_opt: Option<TestOpt>,
    pub check_opt: Option<CheckOpt>,
    pub build_opt: Option<BuildOpt>,
    pub sort_input: bool,
    pub run_mode: RunMode,
    pub fmt_opt: Option<FmtOpt>,
    pub args: Vec<String>,
    pub verbose: bool,
    pub quiet: bool,
    pub no_render_output: bool,
    pub no_parallelize: bool,
    pub build_graph: bool,
    /// Max parallel tasks to run in n2; `None` to use default
    pub parallelism: Option<usize>,
    pub use_tcc_run: bool,
    pub dynamic_stub_libs: Option<Vec<String>>,
    pub render_no_loc: DiagnosticLevel,
}

#[derive(Debug, Clone)]
pub struct BuildOpt {
    pub install_path: Option<PathBuf>,

    pub filter_package: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CheckOpt {
    pub package_name_filter: Option<String>,
    pub patch_file: Option<PathBuf>,
    pub no_mi: bool,
    pub explain: bool,
}

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

#[derive(Debug, Clone)]
pub struct TestOpt {
    pub filter_package: Option<HashSet<String>>,
    pub filter_file: Option<String>,
    pub filter_index: Option<TestIndexRange>,
    pub filter_doc_index: Option<u32>,
    pub limit: u32,
    pub test_failure_json: bool,
    pub display_backend_hint: Option<()>, // use Option to avoid if else
    pub patch_file: Option<PathBuf>,
    /// Glob pattern to filter tests by name
    pub filter_name: Option<String>,
}

#[derive(serde::Serialize, Clone)]
pub struct TestArtifacts {
    pub artifacts_path: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test_filter_args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FmtOpt {
    pub check: bool,
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MooncOpt {
    pub build_opt: BuildPackageFlags,
    pub link_opt: LinkCoreFlags,
    pub extra_build_opt: Vec<String>,
    pub extra_link_opt: Vec<String>,
    pub nostd: bool,
    pub json_diagnostics: bool,
    pub single_file: bool,
}

impl Default for MooncOpt {
    fn default() -> Self {
        Self::new()
    }
}

impl MooncOpt {
    pub fn new() -> Self {
        Self {
            build_opt: BuildPackageFlags::new(),
            link_opt: LinkCoreFlags::new(),
            extra_build_opt: vec![],
            extra_link_opt: vec![],
            nostd: false,
            json_diagnostics: true,
            single_file: false,
        }
    }
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
