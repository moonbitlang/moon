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
use std::path::{Path, PathBuf};

use moonutil::common::TargetBackend;

use crate::build_lower::compiler::{
    BuildCommonArgs, CmdlineAbstraction, CompilationFlags, MiDependency,
};
use crate::model::TargetKind;

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
    pub common: BuildCommonArgs<'a>,

    pub core_out: Cow<'a, Path>,
    #[allow(unused)]
    pub mi_out: Cow<'a, Path>,
    pub no_mi: bool,
    pub flags: CompilationFlags,
    pub extra_build_opts: &'a [&'a str],
}

impl<'a> MooncBuildPackage<'a> {
    #[allow(clippy::too_many_arguments)]
    /// Create a new instance with only necessary fields populated, others as default
    pub fn new(
        mbt_sources: &'a [PathBuf],
        core_out: impl Into<Cow<'a, Path>>,
        mi_out: impl Into<Cow<'a, Path>>,
        mi_deps: &'a [MiDependency<'a>],
        package_name: super::CompiledPackageName<'a>,
        package_source: impl Into<Cow<'a, Path>>,
        target_backend: TargetBackend,
        target_kind: TargetKind,
    ) -> Self {
        Self {
            common: BuildCommonArgs::new(
                mbt_sources,
                mi_deps,
                package_name,
                package_source,
                target_backend,
                target_kind,
            ),
            core_out: core_out.into(),
            mi_out: mi_out.into(),
            no_mi: false,
            flags: CompilationFlags {
                no_opt: false,
                symbols: false,
                source_map: false,
                enable_coverage: false,
                self_coverage: false,
                enable_value_tracing: false,
            },
            extra_build_opts: &[],
        }
    }

    /// Convert this to list of args. The behavior tries to mimic the legacy
    /// behavior as much as possible.
    pub fn to_args_legacy(&self, args: &mut Vec<String>) {
        args.push("build-package".into());

        // Error format
        self.common.add_error_format(args);

        // Warning and alert handling (separate)
        self.common.add_warn_alert_deny_all_separate(args);

        // Input files
        self.common.add_mbt_sources(args);

        // Custom warning/alert lists
        self.common.add_custom_warn_alert_lists(args);

        // Third-party package handling (allow all)
        self.common.add_warn_alert_allow_all(args);

        // Output
        args.extend(["-o".to_string(), self.core_out.display().to_string()]);

        // Package configuration
        self.common.add_package_config(args);

        // is-main (no condition)
        self.common.add_is_main(args);

        // Standard library
        self.common.add_stdlib_path(args);

        // MI dependencies
        self.common.add_mi_dependencies(args);

        // Package source definition
        self.common.add_package_sources(args);

        // Target backend
        self.common.add_target_backend(args);

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

        self.common.add_test_args(args);

        // Virtual package check
        self.common.add_virtual_package_check(args);

        // Virtual package check with no-mi flag
        if self.common.check_mi.is_some() && self.no_mi {
            args.push("-no-mi".to_string());
        }

        // Virtual package implementation
        self.common.add_virtual_package_implementation_build(args);

        self.common.add_test_mode_args(args);

        // Workspace root
        self.common.add_workspace_root(args);
    }
}

impl<'a> CmdlineAbstraction for MooncBuildPackage<'a> {
    fn to_args(&self, args: &mut Vec<String>) {
        self.to_args_legacy(args);
    }
}
