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
/// moving command generation from `gen_bundle_all` into a command abstraction.
#[derive(Debug)]
pub(crate) struct MooncBundleCore<'a> {
    /// Core dependencies (input .core files) to be bundled
    pub core_deps: &'a [PathBuf],
    /// Output path for the bundled .core file
    pub output_path: Cow<'a, Path>,
    /// Extra arguments to pass to the command
    pub extra_args: &'a [&'a str],
}

impl<'a> MooncBundleCore<'a> {
    /// Create a new instance with only necessary fields populated, others as default
    pub(crate) fn new(core_deps: &'a [PathBuf], output_path: impl Into<Cow<'a, Path>>) -> Self {
        Self {
            core_deps,
            output_path: output_path.into(),
            extra_args: &[],
        }
    }
}

impl<'a> CmdlineAbstraction for MooncBundleCore<'a> {
    fn to_args(&self, args: &mut Vec<String>) {
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
