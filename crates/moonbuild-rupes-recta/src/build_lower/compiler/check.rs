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

use crate::build_lower::compiler::{BuildCommonConfig, BuildCommonInput, CmdlineAbstraction};

/// Abstraction for `moonc check`.
///
/// This command checks MoonBit source files for errors and warnings without
/// producing any output `.core` files. It outputs a `.mi` file that contains
/// the public interface information for downstream consumption.
///
/// FIXME: This is a shallow abstraction that still contains some suboptimal
/// abstractions, which needs to be fixed in the future.
#[derive(Debug)]
pub struct MooncCheck<'a> {
    // Common arguments
    pub required: BuildCommonInput<'a>,
    pub defaults: BuildCommonConfig<'a>,

    /// The output `.mi` file's path.
    pub mi_out: Cow<'a, Path>,

    /// Whether to enable single file mode.
    pub single_file: bool,

    /// Extra flags to append at the end.
    pub extra_flags: &'a [String],
}

impl<'a> CmdlineAbstraction for MooncCheck<'a> {
    fn to_args(&self, args: &mut Vec<String>) {
        args.push("check".into());

        // Patch file (first if present)
        self.defaults.add_patch_file_moonc(args);

        // No MI flag
        self.defaults.add_no_mi(args);

        // Error format
        self.defaults.add_error_format(args);

        // Warning and alert handling (deny all combined)
        self.defaults.add_deny_all(args);

        // MBT source files
        self.required.add_mbt_sources(args);

        // Doctest-only MBT files
        self.required.add_doctest_only_sources(args);

        // Include doctests for blackbox
        self.required.add_include_doctests_if_blackbox(args);

        // Custom warning/alert lists
        self.defaults.add_custom_warn_alert_lists(args);
        self.defaults.add_warn_alert_allow_all(args);

        // Output
        if !self.defaults.no_mi {
            args.extend(["-o".to_string(), self.mi_out.display().to_string()]);
        }

        // Package configuration
        self.required.add_package_config(args);

        // is-main
        self.defaults.add_is_main(args);

        // Single file mode
        if self.single_file {
            args.push("-single-file".to_string());
        }

        // Standard library
        self.defaults.add_stdlib_path(args);

        // MI dependencies
        self.required.add_mi_dependencies(args);

        // Package source definition
        self.required.add_package_sources(args);

        // Target backend
        self.required.add_target_backend(args);

        // Test kind flags
        self.required.add_test_kind_flags(args);

        // Virtual package check
        self.defaults.add_virtual_package_check(args);

        // Virtual package implementation
        self.defaults.add_virtual_package_implementation_check(args);

        self.defaults.add_workspace_root(args);

        // all-pkgs.json
        self.required.add_all_pkgs_json(args);

        // Extra flags
        for flag in self.extra_flags.iter() {
            args.push(flag.to_string());
        }
    }
}
