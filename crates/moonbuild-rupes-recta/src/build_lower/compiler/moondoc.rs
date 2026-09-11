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

//! Abstraction for `moondoc`.

use std::borrow::Cow;
use std::path::Path;

use crate::build_lower::compiler::CmdlineAbstraction;

/// Abstraction for `moondoc` documentation generation command.
///
/// `moondoc` reads the dependencies of packages, source files and built `.mi`
/// interface files, and generates documentation in HTML or other formats.
///
/// This struct provides a wrapper around the moondoc command,
/// moving command generation from `run_doc_rr` into a command abstraction.
#[derive(Debug)]
pub(crate) struct MoondocCommand<'a> {
    /// Source directory to generate documentation from
    pub source_dir: Cow<'a, Path>,
    /// Output directory for generated documentation
    pub output_dir: Cow<'a, Path>,
    /// Path to the standard library core bundle
    pub std_path: Option<Cow<'a, Path>>,
    /// Path to the packages.json metadata file
    pub packages_json: Cow<'a, Path>,
    /// Whether to enable serve mode (outputs HTML)
    pub serve_mode: bool,
}

impl<'a> MoondocCommand<'a> {
    /// Create a new instance with only necessary fields populated, others as default
    pub(crate) fn new(
        source_dir: impl Into<Cow<'a, Path>>,
        output_dir: impl Into<Cow<'a, Path>>,
        std_path: Option<impl Into<Cow<'a, Path>>>,
        packages_json: impl Into<Cow<'a, Path>>,
        serve_mode: bool,
    ) -> Self {
        Self {
            source_dir: source_dir.into(),
            output_dir: output_dir.into(),
            std_path: std_path.map(Into::into),
            packages_json: packages_json.into(),
            serve_mode,
        }
    }
}

impl<'a> CmdlineAbstraction for MoondocCommand<'a> {
    fn to_args(&self, args: &mut Vec<String>) {
        // Source directory (positional argument, first)
        args.push(self.source_dir.display().to_string());

        // Output directory
        args.push("-o".to_string());
        args.push(self.output_dir.display().to_string());

        // Standard library path
        if let Some(std_path) = &self.std_path {
            args.push("-std-path".to_string());
            args.push(std_path.display().to_string());
        }

        // Packages metadata file
        args.push("-packages-json".to_string());
        args.push(self.packages_json.display().to_string());

        // Serve mode (optional)
        if self.serve_mode {
            args.push("-serve-mode".to_string());
        }
    }
}
