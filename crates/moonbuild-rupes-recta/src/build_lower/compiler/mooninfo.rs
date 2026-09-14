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

use std::borrow::Cow;
use std::path::Path;

use crate::build_lower::compiler::CmdlineAbstraction;

/// Wrapper for the external `mooninfo` tool.
///
/// `mooninfo` converts a `.mi` or `.core` file into a textual representation.
/// MoonBuild only uses a subset of its functionality to convert `.mi` files
/// into `.mbti` files.
///
/// This mirrors the flag order
/// used by `crates/moon/src/cli/info.rs` when invoking the `mooninfo` binary.
#[derive(Debug)]
pub(crate) struct Mooninfo<'a> {
    /// Input .mi file
    pub mi_in: Cow<'a, Path>,
    /// Output .mbti file path
    pub out: Cow<'a, Path>,
    /// Whether to not emit aliases for package names, and instead use full
    /// package FQNs everywhere.
    pub no_alias: bool,
}

impl<'a> CmdlineAbstraction for Mooninfo<'a> {
    fn to_args(&self, args: &mut Vec<String>) {
        args.push("-format=text".into());

        // 2. input mi path (positional)
        args.push(self.mi_in.display().to_string());

        // 3. output as single -o=<path>
        args.push(format!("-o={}", self.out.display()));

        // 4. optional -no-alias
        if self.no_alias {
            args.push("-no-alias".to_string());
        }
    }
}
