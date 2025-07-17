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

//! Common types and functionality shared between compiler command abstractions

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use moonutil::common::TargetBackend;

use crate::build_lower::compiler::{
    ErrorFormat, MiDependency, VirtualPackageImplementation, WarnAlertConfig,
    MOONC_ALLOW_ALERT_SET, MOONC_ALLOW_WARNING_SET, MOONC_DENY_ALERT_SET, MOONC_DENY_WARNING_SET,
};
use crate::pkg_name::PackageFQN;

/// Common fields shared between different build-like commands of `moonc`
#[derive(Debug)]
pub struct BuildCommonArgs<'a> {
    // Basic command structure
    pub error_format: ErrorFormat,

    // Warning and alert configuration
    pub warn_config: WarnAlertConfig<'a>,
    pub alert_config: WarnAlertConfig<'a>,

    // Input files
    pub mbt_sources: &'a [PathBuf],
    pub mi_deps: &'a [MiDependency<'a>],

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

    // Virtual package handling
    // FIXME: better abstraction
    pub check_mi: Option<Cow<'a, Path>>,
    pub virtual_implementation: Option<VirtualPackageImplementation<'a>>,
}

impl<'a> BuildCommonArgs<'a> {
    /// Create a new instance with default values
    pub fn new(
        mbt_sources: &'a [PathBuf],
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
            mi_deps,
            package_name,
            package_source: package_source.into(),
            is_main: false,
            stdlib_core_file: None,
            target_backend,
            check_mi: None,
            virtual_implementation: None,
        }
    }

    /// Add error format arguments
    pub fn add_error_format(&self, args: &mut Vec<String>) {
        if matches!(self.error_format, ErrorFormat::Json) {
            args.extend(["-error-format".to_string(), "json".to_string()]);
        }
    }

    /// Add MBT source files as arguments
    pub fn add_mbt_sources(&self, args: &mut Vec<String>) {
        for mbt_file in self.mbt_sources {
            args.push(mbt_file.display().to_string());
        }
    }

    /// Add custom warning/alert list arguments
    pub fn add_custom_warn_alert_lists(&self, args: &mut Vec<String>) {
        if let WarnAlertConfig::List(warn_list) = &self.warn_config {
            args.extend(["-w".to_string(), warn_list.to_string()]);
        }
        if let WarnAlertConfig::List(alert_list) = &self.alert_config {
            args.extend(["-alert".to_string(), alert_list.to_string()]);
        }
    }

    /// Add package configuration arguments
    pub fn add_package_config(&self, args: &mut Vec<String>) {
        args.extend(["-pkg".to_string(), self.package_name.to_string()]);
    }

    /// Add is-main flag if applicable
    pub fn add_is_main(&self, args: &mut Vec<String>) {
        if self.is_main {
            args.push("-is-main".to_string());
        }
    }

    /// Add is-main flag with additional condition check
    pub fn add_is_main_with_condition(&self, args: &mut Vec<String>, condition: bool) {
        if self.is_main && condition {
            args.push("-is-main".to_string());
        }
    }

    /// Add standard library path arguments
    pub fn add_stdlib_path(&self, args: &mut Vec<String>) {
        if let Some(stdlib_path) = &self.stdlib_core_file {
            args.extend(["-std-path".to_string(), stdlib_path.display().to_string()]);
        }
    }

    /// Add MI dependencies arguments
    pub fn add_mi_dependencies(&self, args: &mut Vec<String>) {
        for mi_dep in self.mi_deps {
            args.extend(["-i".to_string(), mi_dep.to_alias_arg()]);
        }
    }

    /// Add package source definition arguments
    pub fn add_package_sources(&self, args: &mut Vec<String>) {
        args.extend([
            "-pkg-sources".to_string(),
            format!("{}:{}", self.package_name, self.package_source.display()),
        ]);
    }

    /// Add target backend arguments
    pub fn add_target_backend(&self, args: &mut Vec<String>) {
        args.extend([
            "-target".to_string(),
            self.target_backend.to_flag().to_string(),
        ]);
    }

    /// Add virtual package check arguments
    pub fn add_virtual_package_check(&self, args: &mut Vec<String>) {
        if let Some(check_mi_path) = &self.check_mi {
            args.extend(["-check-mi".to_string(), check_mi_path.display().to_string()]);
        }
    }

    /// Add warning/alert deny all arguments (combined)
    pub fn add_warn_alert_deny_all_combined(&self, args: &mut Vec<String>) {
        if matches!(self.warn_config, WarnAlertConfig::DenyAll) {
            args.extend([
                "-w".to_string(),
                MOONC_DENY_WARNING_SET.to_string(),
                "-alert".to_string(),
                MOONC_DENY_ALERT_SET.to_string(),
            ]);
        }
    }

    /// Add warning/alert deny all arguments (separate)
    pub fn add_warn_alert_deny_all_separate(&self, args: &mut Vec<String>) {
        if matches!(self.warn_config, WarnAlertConfig::DenyAll) {
            args.extend(["-w".to_string(), MOONC_DENY_WARNING_SET.to_string()]);
        }
        if matches!(self.alert_config, WarnAlertConfig::DenyAll) {
            args.extend(["-alert".to_string(), MOONC_DENY_ALERT_SET.to_string()]);
        }
    }

    /// Add warning/alert allow all arguments
    pub fn add_warn_alert_allow_all(&self, args: &mut Vec<String>) {
        if matches!(self.warn_config, WarnAlertConfig::AllowAll) {
            args.extend(["-w".to_string(), MOONC_ALLOW_WARNING_SET.into()]);
        }
        if matches!(self.alert_config, WarnAlertConfig::AllowAll) {
            args.extend(["-alert".to_string(), MOONC_ALLOW_ALERT_SET.into()]);
        }
    }

    /// Add virtual package implementation arguments (with different behavior for check vs build-package)
    pub fn add_virtual_package_implementation_check(&self, args: &mut Vec<String>) {
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

    /// Add virtual package implementation arguments for build-package
    pub fn add_virtual_package_implementation_build(&self, args: &mut Vec<String>) {
        if let Some(impl_virtual) = &self.virtual_implementation {
            args.extend([
                "-check-mi".to_string(),
                impl_virtual.mi_path.display().to_string(),
                "-impl-virtual".to_string(),
                "-no-mi".to_string(),
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
