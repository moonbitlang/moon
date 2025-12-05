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
/// converting from the legacy `run_doc_rr` function implementation
/// to the new command abstraction pattern.
#[derive(Debug)]
pub struct MoondocCommand<'a> {
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
    pub fn new(
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

    /// Convert this to list of args. The behavior mirrors the legacy
    /// `run_doc_rr` function's command generation.
    pub fn to_args_legacy(&self, args: &mut Vec<String>) {
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

impl<'a> CmdlineAbstraction for MoondocCommand<'a> {
    fn to_args(&self, args: &mut Vec<String>) {
        self.to_args_legacy(args);
    }
}
