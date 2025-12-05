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

//! Abstraction for `moonc bundle-core`.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::build_lower::compiler::CmdlineAbstraction;

/// Abstraction for `moonc bundle-core`.
///
/// This command bundles multiple `.core` files into a single `.core` file.
/// It is currently only used in `moonbitlang/core`.
///
/// This struct provides a wrapper around the bundle-core command,
/// converting from the legacy `gen_bundle_all` function implementation
/// to the new command abstraction pattern.
#[derive(Debug)]
pub struct MooncBundleCore<'a> {
    /// Core dependencies (input .core files) to be bundled
    pub core_deps: &'a [PathBuf],
    /// Output path for the bundled .core file
    pub output_path: Cow<'a, Path>,
    /// Extra arguments to pass to the command
    pub extra_args: &'a [&'a str],
}

impl<'a> MooncBundleCore<'a> {
    /// Create a new instance with only necessary fields populated, others as default
    pub fn new(core_deps: &'a [PathBuf], output_path: impl Into<Cow<'a, Path>>) -> Self {
        Self {
            core_deps,
            output_path: output_path.into(),
            extra_args: &[],
        }
    }

    /// Convert this to list of args. The behavior mirrors the legacy
    /// `gen_bundle_all` function's command generation.
    pub fn to_args_legacy(&self, args: &mut Vec<String>) {
        // Command name
        args.push("bundle-core".into());

        // Input core files (in order)
        for core_dep in self.core_deps {
            args.push(core_dep.display().to_string());
        }

        // Output file
        args.push("-o".to_string());
        args.push(self.output_path.display().to_string());

        // Extra arguments
        for arg in self.extra_args {
            args.push(arg.to_string());
        }
    }
}

impl<'a> CmdlineAbstraction for MooncBundleCore<'a> {
    fn to_args(&self, args: &mut Vec<String>) {
        self.to_args_legacy(args);
    }
}
