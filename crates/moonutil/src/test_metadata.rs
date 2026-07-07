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

use std::path::PathBuf;

use clap::ValueEnum;
use indexmap::IndexMap;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub enum GeneratedTestDriver {
    InternalTest(PathBuf),
    WhiteboxTest(PathBuf),
    BlackboxTest(PathBuf),
}

#[derive(Debug, ValueEnum, Clone, Copy)]
pub enum DriverKind {
    Internal,
    Whitebox,
    Blackbox,
}

impl std::fmt::Display for DriverKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Self::Internal => "internal",
            Self::Whitebox => "whitebox",
            Self::Blackbox => "blackbox",
        };
        write!(f, "{kind}")
    }
}

#[derive(Debug, Default, ValueEnum, Clone, PartialEq, Copy, PartialOrd)]
pub enum DiagnosticLevel {
    Info,
    #[value(alias = "warning")]
    Warn,
    #[default]
    Error,
}

impl std::fmt::Display for DiagnosticLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
        })
    }
}

pub const BLACKBOX_TEST_DRIVER: &str = "__generated_driver_for_blackbox_test.mbt";

pub type FileName = String;
pub type TestName = String;
pub type TestBlockIndex = u32;

#[derive(Deserialize, Debug, Clone)]
pub struct MbtTestInfo {
    /// The index of the test block in the file, starting from 0.
    pub index: TestBlockIndex,
    /// The function name of the test block
    pub func: String,
    /// The name of the test block, if any
    pub name: Option<TestName>,
    /// The line number of the definition of the test block, if any
    #[serde(default)]
    pub line_number: Option<usize>,
    /// The attributes of the test block (e.g., #cfg conditions)
    #[serde(default)]
    pub attrs: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MooncGenTestInfo {
    pub no_args_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    pub with_args_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    #[serde(default)] // for backward compatibility
    pub with_bench_args_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    #[serde(default)]
    pub async_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    #[serde(default)]
    pub async_tests_with_args: IndexMap<FileName, Vec<MbtTestInfo>>,
}

impl MbtTestInfo {
    pub fn has_skip(&self) -> bool {
        self.attrs.iter().any(|attr| attr.starts_with("#skip"))
    }
}

impl MooncGenTestInfo {
    /// Convert part of the driver metadata into MoonBit declaraction code for
    /// the test driver to use.
    pub fn section_to_mbt(section: &IndexMap<FileName, Vec<MbtTestInfo>>) -> String {
        use std::fmt::Write;

        let mut result = String::new();
        let default_name = "";

        // Writing to string cannot fail, so unwrap() is safe here.
        writeln!(result, "{{").unwrap();
        for (file, tests) in section {
            writeln!(result, "  \"{file}\": {{").unwrap();
            for test in tests {
                // tests with #skip attribute are also included in the driver, they will
                // eventually be skipped by using cli arguments to the driver executable
                writeln!(
                    result,
                    "    {}: ({}, [\"{}\"]),",
                    test.index,
                    test.func,
                    test.name.as_deref().unwrap_or(default_name)
                )
                .unwrap();
            }
            writeln!(result, "  }},").unwrap();
        }
        writeln!(result, "}}").unwrap();

        result
    }
}
