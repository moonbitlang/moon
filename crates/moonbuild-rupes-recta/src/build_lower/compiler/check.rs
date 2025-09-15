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

//! Compiler command abstraction

use std::borrow::Cow;
use std::path::Path;

use crate::build_lower::compiler::{BuildCommonArgs, CmdlineAbstraction};

/// Abstraction for `moonc check`.
///
/// FIXME: This is a shallow abstraction that tries to mimic the legacy
/// behavior as much as possible. It definitely contains some suboptimal
/// abstractions, which needs to be fixed in the future.
///
/// FIXME: Avoid laying everything out flat
#[derive(Debug)]
pub struct MooncCheck<'a> {
    // Common arguments
    pub common: BuildCommonArgs<'a>,

    pub mi_out: Cow<'a, Path>,
    pub no_mi: bool,

    pub is_third_party: bool,
    pub single_file: bool,
    pub patch_file: Option<Cow<'a, Path>>,
}

impl<'a> MooncCheck<'a> {
    /// Convert this to list of args. The behavior tries to mimic the legacy
    /// behavior as much as possible.
    pub fn to_args_legacy(&self, args: &mut Vec<String>) {
        args.push("check".into());

        // Patch file (first if present)
        if let Some(patch_file) = &self.patch_file {
            args.extend(["-patch-file".to_string(), patch_file.display().to_string()]);
        }

        // No MI flag
        if self.no_mi {
            args.push("-no-mi".to_string());
        }

        // Error format
        self.common.add_error_format(args);

        // Warning and alert handling (deny all combined)
        self.common.add_warn_alert_deny_all_combined(args);

        // MBT source files
        self.common.add_mbt_sources(args);

        // Doctest-only MBT files
        self.common.add_doctest_only_sources(args);

        // Custom warning/alert lists
        self.common.add_custom_warn_alert_lists(args);

        // Third-party package handling
        if self.is_third_party {
            args.extend([
                "-w".to_string(),
                "-a".to_string(),
                "-alert".to_string(),
                "-all".to_string(),
            ]);
        }

        // Output
        args.extend(["-o".to_string(), self.mi_out.display().to_string()]);

        // Package configuration
        self.common.add_package_config(args);

        // is-main with blackbox test condition
        self.common.add_is_main(args);

        // Single file mode
        if self.single_file {
            args.push("-single-file".to_string());
        }

        // Standard library
        self.common.add_stdlib_path(args);

        // MI dependencies
        self.common.add_mi_dependencies(args);

        // Package source definition
        self.common.add_package_sources(args);

        // Target backend
        self.common.add_target_backend(args);

        // Test type flags
        self.common.add_test_args(args);

        // Virtual package check
        self.common.add_virtual_package_check(args);

        // Virtual package implementation
        self.common.add_virtual_package_implementation_check(args);

        // Workspace root
        self.common.add_workspace_root(args);
    }
}

impl<'a> CmdlineAbstraction for MooncCheck<'a> {
    fn to_args(&self, args: &mut Vec<String>) {
        self.to_args_legacy(args);
    }
}
