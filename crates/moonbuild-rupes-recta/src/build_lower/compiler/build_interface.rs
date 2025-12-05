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

//! Abstraction for `moonc build-interface`.

use std::borrow::Cow;
use std::path::Path;

use crate::build_lower::compiler::{CmdlineAbstraction, CompiledPackageName, MiDependency};

/// Command-line abstraction for `moonc build-interface`.
///
/// This builds a MoonBit interface file (`.mi`) from a MoonBit interface
/// contract file (`.mbti`). It is the reverse of `mooninfo`.
///
/// This mirrors the invocation assembled in
/// [`gen_build_interface_command`](crates/moonbuild/src/gen/gen_build.rs:369) and
/// is used to parametrise interface generation for virtual packages.
#[derive(Debug)]
pub struct MooncBuildInterface<'a> {
    /// The source `.mbti` contract.
    pub mbti_input: Cow<'a, Path>,
    /// Destination `.mi` path.
    pub mi_output: Cow<'a, Path>,
    /// Interface dependencies (each rendered as `<path>:<alias>`).
    pub mi_deps: &'a [MiDependency<'a>],
    /// Fully-qualified package name.
    pub package_name: CompiledPackageName<'a>,
    /// Absolute source directory for the package.
    pub package_source: Cow<'a, Path>,
    /// Optional std path; omit to respect `--nostd`.
    pub stdlib_core_file: Option<Cow<'a, Path>>,
    /// Whether to emit structured diagnostics (`-error-format=json`).
    pub json_errors: bool,
}

impl<'a> MooncBuildInterface<'a> {
    /// Create a new command abstraction with defaults matching the legacy builder.
    pub fn new(
        mbti_input: impl Into<Cow<'a, Path>>,
        mi_output: impl Into<Cow<'a, Path>>,
        mi_deps: &'a [MiDependency<'a>],
        package_name: CompiledPackageName<'a>,
        package_source: impl Into<Cow<'a, Path>>,
    ) -> Self {
        Self {
            mbti_input: mbti_input.into(),
            mi_output: mi_output.into(),
            mi_deps,
            package_name,
            package_source: package_source.into(),
            stdlib_core_file: None,
            json_errors: true,
        }
    }
}

impl CmdlineAbstraction for MooncBuildInterface<'_> {
    fn to_args(&self, args: &mut Vec<String>) {
        args.push("build-interface".into());

        // Input and output paths
        args.push(self.mbti_input.display().to_string());
        args.extend(["-o".to_string(), self.mi_output.display().to_string()]);

        // Interface dependencies
        for dep in self.mi_deps {
            args.extend(["-i".to_string(), dep.to_alias_arg()]);
        }

        // Package metadata
        args.extend(["-pkg".to_string(), self.package_name.to_string()]);
        args.extend([
            "-pkg-sources".to_string(),
            format!("{}:{}", self.package_name, self.package_source.display()),
        ]);

        // Virtual packages always require this flag
        args.push("-virtual".to_string());

        if let Some(stdlib) = &self.stdlib_core_file {
            args.extend(["-std-path".to_string(), stdlib.display().to_string()]);
        }

        if self.json_errors {
            args.extend(["-error-format".to_string(), "json".to_string()]);
        }
    }
}
