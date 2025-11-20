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

use crate::build_lower::compiler::{
    BuildCommonConfig, BuildCommonInput, CmdlineAbstraction, CompilationFlags,
};

/// Abstraction for `moonc build-package`.
///
/// FIXME: This is a shallow abstraction that tries to mimic the legacy
/// behavior as much as possible. It definitely contains some suboptimal
/// abstractions, which needs to be fixed in the future.
///
/// FIXME: Avoid laying everything out flat
#[derive(Debug)]
pub struct MooncBuildPackage<'a> {
    // Common arguments
    pub required: BuildCommonInput<'a>,
    pub defaults: BuildCommonConfig<'a>,
    pub core_out: Cow<'a, Path>,
    #[allow(unused)]
    pub mi_out: Cow<'a, Path>,

    pub flags: CompilationFlags,
    pub extra_build_opts: &'a [String],
}

impl<'a> MooncBuildPackage<'a> {
    /// Convert this to list of args. The behavior tries to mimic the legacy
    /// behavior as much as possible.
    pub fn to_args_legacy(&self, args: &mut Vec<String>) {
        args.push("build-package".into());

        // Error format
        self.defaults.add_error_format(args);

        // Warning and alert handling
        self.defaults.add_deny_all(args);

        // Input files
        self.required.add_mbt_sources(args);
        // Additional inputs following legacy ordering
        self.required.add_doctest_only_sources(args);

        // Custom warning/alert lists
        self.defaults.add_custom_warn_alert_lists(args);

        // Third-party package handling (allow all)
        self.defaults.add_warn_alert_allow_all(args);

        // Output
        args.extend(["-o".to_string(), self.core_out.display().to_string()]);

        // Package configuration
        self.required.add_package_config(args);

        // is-main (no condition)
        self.defaults.add_is_main(args);

        // Standard library
        self.defaults.add_stdlib_path(args);

        // MI dependencies
        self.required.add_mi_dependencies(args);

        // Package source definition
        self.required.add_package_sources(args);

        // Target backend
        self.required.add_target_backend(args);

        // Debug and optimization flags
        if self.flags.symbols {
            args.push("-g".to_string());
        }
        if self.flags.no_opt {
            args.push("-O0".to_string());
        }

        // Additional compilation flags
        if self.flags.source_map {
            args.push("-source-map".to_string());
        }
        if self.flags.enable_coverage {
            args.push("-enable-coverage".to_string());
        }
        if self.flags.self_coverage {
            args.push("-coverage-package-override=@self".to_string());
        }
        if self.flags.enable_value_tracing {
            args.push("-enable-value-tracing".to_string());
        }

        // Extra build options
        for opt in self.extra_build_opts {
            args.push(opt.to_string());
        }

        self.required.add_test_args(args);

        // Virtual package check
        self.defaults.add_virtual_package_check(args);

        // Virtual package implementation
        self.defaults.add_virtual_package_implementation_build(args);

        // -no-mi after test flags
        self.defaults.add_no_mi(args);

        // patch after -no-mi
        self.defaults.add_patch_file_moonc(args);

        self.required.add_test_mode_args(args);

        // -enable-value-tracing after test mode flags
        self.defaults.add_enable_value_tracing(args);

        // Workspace root
        self.defaults.add_workspace_root(args);
    }
}

impl<'a> CmdlineAbstraction for MooncBuildPackage<'a> {
    fn to_args(&self, args: &mut Vec<String>) {
        self.to_args_legacy(args);
    }
}
