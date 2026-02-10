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
pub struct Mooninfo<'a> {
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
