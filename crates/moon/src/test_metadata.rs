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

//! Compiler-produced test metadata shared by driver generation and test execution.

use indexmap::IndexMap;
use serde::Deserialize;

pub(crate) type FileName = String;
pub(crate) type TestName = String;
pub(crate) type TestBlockIndex = u32;

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct MbtTestInfo {
    /// The index of the test block in the file, starting from 0.
    pub(crate) index: TestBlockIndex,
    /// The function name of the test block
    pub(crate) func: String,
    /// The name of the test block, if any
    pub(crate) name: Option<TestName>,
    /// The line number of the definition of the test block, if any
    #[serde(default)]
    pub(crate) line_number: Option<usize>,
    /// The attributes of the test block (e.g., #cfg conditions)
    #[serde(default)]
    pub(crate) attrs: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct MooncGenTestInfo {
    pub(crate) no_args_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    pub(crate) with_args_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    #[serde(default)] // for backward compatibility
    pub(crate) with_bench_args_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    #[serde(default)]
    pub(crate) async_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    #[serde(default)]
    pub(crate) async_tests_with_args: IndexMap<FileName, Vec<MbtTestInfo>>,
}

impl MbtTestInfo {
    pub(crate) fn has_skip(&self) -> bool {
        self.attrs.iter().any(|attr| attr.starts_with("#skip"))
    }
}
