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
use std::path::{Path, PathBuf};

use moonutil::common::{DriverKind, TargetBackend};

use crate::build_lower::compiler::CmdlineAbstraction;

/// Commandline arguments to `moon generate-test-driver`.
///
/// Check `moon::cli::generate_test_driver` for details. This struct is a by-ref
/// mirror of the original command for process spawning.
#[derive(Debug)]
pub struct MoonGenTestDriver<'a> {
    /// The paths of the source files to be mapped
    pub files: &'a [PathBuf],

    /// Files that need to be mapped, but only extract the doctests, not main contents
    pub doctest_only_files: &'a [PathBuf],

    /// The output test driver `.mbt` file
    pub output_driver: Cow<'a, Path>,

    /// The output test metadata file
    pub output_metadata: Cow<'a, Path>,

    /// The target backend for the generated test driver.
    pub target_backend: TargetBackend,

    /// The name of the package for which the test driver is generated for.
    pub pkg_name: &'a str,

    /// Whether to generate the test driver in bench mode. Not providing this
    /// option will result in test mode.
    pub bench: bool,

    /// Whether coverage is enabled in this build. Enabling it will insert
    /// coverage-custom code at the end of the test.
    pub enable_coverage: bool,

    /// Override coverage package name; `@self` is a special value that means the package itself
    pub coverage_package_override: Option<&'a str>,

    /// The test driver kind
    pub driver_kind: DriverKind,

    /// Path to the patch file
    pub patch_file: Option<Cow<'a, Path>>,
}

impl<'a> CmdlineAbstraction for MoonGenTestDriver<'a> {
    fn to_args(&self, args: &mut Vec<String>) {
        args.push("generate-test-driver".into());

        // Output files
        args.extend([
            "--output-driver".to_string(),
            self.output_driver.display().to_string(),
        ]);
        args.extend([
            "--output-metadata".to_string(),
            self.output_metadata.display().to_string(),
        ]);

        // Input files
        for file in self.files {
            args.push(file.display().to_string());
        }

        // Doctest-only files
        for file in self.doctest_only_files {
            args.extend(["--doctest-only".to_string(), file.display().to_string()]);
        }

        // Patch file
        if let Some(patch_file) = &self.patch_file {
            args.extend(["--patch-file".to_string(), patch_file.display().to_string()]);
        }

        // Configuration
        args.extend([
            "--target".to_string(),
            self.target_backend.to_flag().to_string(),
        ]);
        args.extend(["--pkg-name".to_string(), self.pkg_name.to_string()]);

        // Bench mode
        if self.bench {
            args.push("--bench".to_string());
        }

        // Coverage arguments
        if self.enable_coverage {
            args.push("--enable-coverage".to_string());
        }
        if let Some(coverage_override) = self.coverage_package_override {
            args.extend([
                "--coverage-package-override".to_string(),
                coverage_override.to_string(),
            ]);
        }

        // Driver kind
        args.extend(["--driver-kind".to_string(), self.driver_kind.to_string()]);
    }
}
