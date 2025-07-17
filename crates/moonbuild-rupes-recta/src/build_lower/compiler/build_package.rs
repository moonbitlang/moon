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
    CmdlineAbstraction, CompilationFlags, ErrorFormat, MiDependency, VirtualPackageImplementation,
    WarnAlertConfig, MOONC_ALLOW_ALERT_SET, MOONC_ALLOW_WARNING_SET, MOONC_DENY_ALERT_SET,
    MOONC_DENY_WARNING_SET,
};
use crate::pkg_name::PackageFQN;

/// Abstraction for `moonc build-package`.
///
/// FIXME: This is a shallow abstraction that tries to mimic the legacy
/// behavior as much as possible. It definitely contains some suboptimal
/// abstractions, which needs to be fixed in the future.
///
/// FIXME: Avoid laying everything out flat
#[derive(Debug)]
pub struct MooncBuildPackage<'a> {
    // Basic command structure
    pub error_format: ErrorFormat,

    // Warning and alert configuration
    pub warn_config: WarnAlertConfig<'a>,
    pub alert_config: WarnAlertConfig<'a>,

    // Input files
    pub mbt_sources: &'a [PathBuf],
    pub mi_deps: &'a [MiDependency<'a>],

    // Output configuration
    pub core_out: Cow<'a, Path>,
    #[allow(unused)]
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

    // Compilation flags
    pub flags: CompilationFlags,

    // Extra options
    pub extra_build_opts: &'a [&'a str],

    // Virtual package handling
    // FIXME: better abstraction
    pub check_mi: Option<Cow<'a, Path>>,
    pub virtual_implementation: Option<VirtualPackageImplementation<'a>>,
}

impl<'a> MooncBuildPackage<'a> {
    /// Create a new instance with only necessary fields populated, others as default
    pub fn new(
        mbt_sources: &'a [PathBuf],
        core_out: impl Into<Cow<'a, Path>>,
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
            mi_deps,
            core_out: core_out.into(),
            mi_out: mi_out.into(),
            no_mi: false,
            package_name,
            package_source: package_source.into(),
            is_main: false,
            stdlib_core_file: None,
            target_backend,
            flags: CompilationFlags {
                no_opt: false,
                symbols: false,
                source_map: false,
                enable_coverage: false,
                self_coverage: false,
                enable_value_tracing: false,
            },
            extra_build_opts: &[],
            check_mi: None,
            virtual_implementation: None,
        }
    }

    /// Convert this to list of args. The behavior tries to mimic the legacy
    /// behavior as much as possible.
    pub fn to_args_legacy(&self, args: &mut Vec<String>) {
        args.push("build-package".into());

        // Error format
        if matches!(self.error_format, ErrorFormat::Json) {
            args.extend(["-error-format".to_string(), "json".to_string()]);
        }

        // Warning and alert handling
        if matches!(self.warn_config, WarnAlertConfig::DenyAll) {
            args.extend(["-w".to_string(), MOONC_DENY_WARNING_SET.to_string()]);
        }
        if matches!(self.alert_config, WarnAlertConfig::DenyAll) {
            args.extend(["-alert".to_string(), MOONC_DENY_ALERT_SET.to_string()]);
        }

        // Input files
        for mbt_file in self.mbt_sources {
            args.push(mbt_file.display().to_string());
        }

        // Custom warning/alert lists
        if let WarnAlertConfig::List(warn_list) = &self.warn_config {
            args.extend(["-w".to_string(), warn_list.to_string()]);
        }
        if let WarnAlertConfig::List(alert_list) = &self.alert_config {
            args.extend(["-alert".to_string(), alert_list.to_string()]);
        }
        // Third-party package handling
        if matches!(self.warn_config, WarnAlertConfig::AllowAll) {
            args.extend(["-w".to_string(), MOONC_ALLOW_WARNING_SET.into()]);
        }
        if matches!(self.alert_config, WarnAlertConfig::AllowAll) {
            args.extend(["-alert".to_string(), MOONC_ALLOW_ALERT_SET.into()]);
        }

        // Output
        args.extend(["-o".to_string(), self.core_out.display().to_string()]);

        // Package configuration
        args.extend(["-pkg".to_string(), self.package_name.to_string()]);

        if self.is_main {
            args.push("-is-main".to_string());
        }

        // Standard library
        if let Some(stdlib_path) = &self.stdlib_core_file {
            args.extend(["-std-path".to_string(), stdlib_path.display().to_string()]);
        }

        // MI dependencies
        for mi_dep in self.mi_deps {
            args.extend(["-i".to_string(), mi_dep.to_alias_arg()]);
        }

        // self package source definition
        args.extend([
            "-pkg-sources".to_string(),
            format!("{}:{}", self.package_name, self.package_source.display()),
        ]);

        // Target backend
        args.extend([
            "-target".to_string(),
            self.target_backend.to_flag().to_string(),
        ]);

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

        // Virtual package check
        if let Some(check_mi_path) = &self.check_mi {
            args.extend(["-check-mi".to_string(), check_mi_path.display().to_string()]);

            if self.no_mi {
                args.push("-no-mi".to_string());
            }
        }

        // Virtual package implementation
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

impl<'a> CmdlineAbstraction for MooncBuildPackage<'a> {
    fn to_args(&self, args: &mut Vec<String>) {
        self.to_args_legacy(args);
    }
}
