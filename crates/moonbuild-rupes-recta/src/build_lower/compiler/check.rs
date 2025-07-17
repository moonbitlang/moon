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
    CmdlineAbstraction, ErrorFormat, MiDependency, VirtualPackageImplementation, WarnAlertConfig,
    MOONC_DENY_ALERT_SET, MOONC_DENY_WARNING_SET,
};
use crate::pkg_name::PackageFQN;

/// Abstraction for `moonc check`.
///
/// FIXME: This is a shallow abstraction that tries to mimic the legacy
/// behavior as much as possible. It definitely contains some suboptimal
/// abstractions, which needs to be fixed in the future.
///
/// FIXME: Avoid laying everything out flat
#[derive(Debug)]
pub struct MooncCheck<'a> {
    // Basic command structure
    pub error_format: ErrorFormat,

    // Warning and alert configuration
    pub warn_config: WarnAlertConfig<'a>,
    pub alert_config: WarnAlertConfig<'a>,

    // Input files
    pub mbt_sources: &'a [PathBuf],
    pub doctest_only_mbt_sources: &'a [PathBuf],
    pub mbt_md_sources: &'a [PathBuf],
    pub mi_deps: &'a [MiDependency<'a>],

    // Output configuration
    pub mi_out: Cow<'a, Path>,
    pub no_mi: bool,

    // Package configuration
    /// The name of the current package
    pub package_name: &'a PackageFQN,
    /// The source directory of the current package
    pub package_source: Cow<'a, Path>,
    pub is_main: bool,

    // Standard library
    /// Pass [None] for no_std
    pub stdlib_core_file: Option<Cow<'a, Path>>,

    // Target configuration
    pub target_backend: TargetBackend,

    // Test configuration
    pub is_whitebox_test: bool,
    pub is_blackbox_test: bool,

    // Third party configuration
    pub is_third_party: bool,

    // Single file mode
    pub single_file: bool,

    // Patch file support
    pub patch_file: Option<Cow<'a, Path>>,

    // Virtual package handling
    // FIXME: better abstraction
    pub check_mi: Option<Cow<'a, Path>>,
    pub virtual_implementation: Option<VirtualPackageImplementation<'a>>,
}

impl<'a> MooncCheck<'a> {
    /// Create a new instance with only necessary fields populated, others as default
    pub fn new(
        mbt_sources: &'a [PathBuf],
        mi_out: impl Into<Cow<'a, Path>>,
        mi_deps: &'a [MiDependency<'a>],
        package_name: &'a PackageFQN,
        package_source: impl Into<Cow<'a, Path>>,
        target_backend: TargetBackend,
    ) -> Self {
        Self {
            error_format: ErrorFormat::Regular,
            warn_config: WarnAlertConfig::Default,
            alert_config: WarnAlertConfig::Default,
            mbt_sources,
            doctest_only_mbt_sources: &[],
            mbt_md_sources: &[],
            mi_deps,
            mi_out: mi_out.into(),
            no_mi: false,
            package_name,
            package_source: package_source.into(),
            is_main: false,
            stdlib_core_file: None,
            target_backend,
            is_whitebox_test: false,
            is_blackbox_test: false,
            is_third_party: false,
            single_file: false,
            patch_file: None,
            check_mi: None,
            virtual_implementation: None,
        }
    }

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
        if matches!(self.error_format, ErrorFormat::Json) {
            args.extend(["-error-format".to_string(), "json".to_string()]);
        }

        // Warning and alert handling (deny all)
        if matches!(self.warn_config, WarnAlertConfig::DenyAll) {
            args.extend([
                "-w".to_string(),
                MOONC_DENY_WARNING_SET.to_string(),
                "-alert".to_string(),
                MOONC_DENY_ALERT_SET.to_string(),
            ]);
        }

        // MBT source files
        for mbt_file in self.mbt_sources {
            args.push(mbt_file.display().to_string());
        }

        // MBT.md files
        for mbt_md_file in self.mbt_md_sources {
            args.push(mbt_md_file.display().to_string());
        }

        // Doctest-only MBT files
        for doctest_file in self.doctest_only_mbt_sources {
            args.extend([
                "-doctest-only".to_string(),
                doctest_file.display().to_string(),
            ]);
        }

        // Include doctests for blackbox tests
        if self.is_blackbox_test {
            args.push("-include-doctests".to_string());
        }

        // Custom warning/alert lists
        if let WarnAlertConfig::List(warn_list) = &self.warn_config {
            args.extend(["-w".to_string(), warn_list.to_string()]);
        }
        if let WarnAlertConfig::List(alert_list) = &self.alert_config {
            args.extend(["-alert".to_string(), alert_list.to_string()]);
        }

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
        args.extend(["-pkg".to_string(), self.package_name.to_string()]);

        if self.is_main && !self.is_blackbox_test {
            args.push("-is-main".to_string());
        }

        // Single file mode
        if self.single_file {
            args.push("-single-file".to_string());
        }

        // Standard library
        if let Some(stdlib_path) = &self.stdlib_core_file {
            args.extend(["-std-path".to_string(), stdlib_path.display().to_string()]);
        }

        // MI dependencies
        for mi_dep in self.mi_deps {
            args.extend(["-i".to_string(), mi_dep.to_alias_arg()]);
        }

        // Package source definition
        args.extend([
            "-pkg-sources".to_string(),
            format!("{}:{}", self.package_name, self.package_source.display()),
        ]);

        // Target backend
        args.extend([
            "-target".to_string(),
            self.target_backend.to_flag().to_string(),
        ]);

        // Test type flags
        if self.is_whitebox_test {
            args.push("-whitebox-test".to_string());
        }
        if self.is_blackbox_test {
            args.push("-blackbox-test".to_string());
        }

        // Virtual package check
        if let Some(check_mi_path) = &self.check_mi {
            args.extend(["-check-mi".to_string(), check_mi_path.display().to_string()]);
        }

        // Virtual package implementation
        if let Some(impl_virtual) = &self.virtual_implementation {
            args.extend([
                "-check-mi".to_string(),
                impl_virtual.mi_path.display().to_string(),
                "-pkg-sources".to_string(),
                format!(
                    "{}:{}",
                    impl_virtual.package_name,
                    impl_virtual.package_path.display()
                ),
            ]);
        }
    }
}

impl<'a> CmdlineAbstraction for MooncCheck<'a> {
    fn to_args(&self, args: &mut Vec<String>) {
        self.to_args_legacy(args);
    }
}
