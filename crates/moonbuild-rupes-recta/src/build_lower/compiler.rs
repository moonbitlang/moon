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
use moonutil::package::{ImportMemory, JsFormat, MemoryLimits};

use crate::pkg_name::PackageFQN;

#[allow(unused)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorFormat {
    Regular,
    Json,
}

#[derive(Clone, Debug)]
pub struct MiDependency<'a> {
    pub path: Cow<'a, Path>,
    pub alias: Option<Cow<'a, str>>,
}

impl<'a> MiDependency<'a> {
    pub fn to_alias_arg(&self) -> String {
        if let Some(alias) = &self.alias {
            format!("{}:{}", self.path.display(), alias)
        } else {
            format!("{}:{}", self.path.display(), self.path.display())
        }
    }

    pub fn new(path: impl Into<Cow<'a, Path>>, alias: impl Into<Cow<'a, str>>) -> Self {
        Self {
            path: path.into(),
            alias: Some(alias.into()),
        }
    }

    #[allow(unused)]
    pub fn no_alias(path: impl Into<Cow<'a, Path>>) -> Self {
        Self {
            path: path.into(),
            alias: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PackageSource<'a> {
    pub package_name: &'a PackageFQN,
    pub source_dir: Cow<'a, Path>,
}

impl<'a> PackageSource<'a> {
    pub fn to_arg(&self) -> String {
        format!("{}:{}", self.package_name, self.source_dir.display())
    }
}

#[derive(Clone, Debug)]
pub struct VirtualPackageImplementation<'a> {
    pub mi_path: Cow<'a, Path>,
    pub package_name: &'a PackageFQN,
    pub package_path: Cow<'a, Path>,
}

#[derive(Clone, Debug)]
pub struct CompilationFlags {
    /// Disable optimization (adds -O0)
    pub no_opt: bool,
    /// Include debug symbols (adds -g)
    pub symbols: bool,
    pub source_map: bool,
    pub enable_coverage: bool,
    pub self_coverage: bool,
    pub enable_value_tracing: bool,
}

/// Configuration for either warning or alert
#[derive(Clone, Debug, Default)]
#[allow(unused)]
pub enum WarnAlertConfig<'a> {
    #[default]
    Default,
    List(Cow<'a, str>),
    DenyAll,
    AllowAll,
}

pub trait CmdlineAbstraction {
    fn to_args(&self, args: &mut Vec<String>);
}

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

#[allow(unused)]
const MOONC_REGULAR_WARNING_SET: &str = "+a-31-32";
#[allow(unused)]
const MOONC_REGULAR_ALERT_SET: &str = "+all-raise-throw-unsafe+deprecated";

const MOONC_DENY_WARNING_SET: &str = "@a-31-32";
const MOONC_DENY_ALERT_SET: &str = "@all-raise-throw-unsafe+deprecated";
const MOONC_ALLOW_WARNING_SET: &str = "-a";
const MOONC_ALLOW_ALERT_SET: &str = "-all";

/// Abstraction for `moonc link-core`.
///
/// This struct reuses existing structures and mimics the legacy behavior
/// as much as possible, maintaining EXACT argument order.
#[derive(Debug)]
pub struct MooncLinkCore<'a> {
    // Input/Output configuration
    pub core_deps: &'a [PathBuf],
    pub main_package: &'a PackageFQN,
    pub output_path: Cow<'a, Path>,
    pub pkg_config_path: Cow<'a, Path>,

    // Package configuration
    pub package_sources: &'a [PackageSource<'a>],
    /// Pass [None] for no_std, otherwise provide PackageSource for core
    pub stdlib_core_source: Option<PackageSource<'a>>,

    // Target and compilation configuration
    pub target_backend: TargetBackend,
    /// Compilation flags - reuse existing structure, symbols maps to -g, no_opt maps to -O0
    pub flags: CompilationFlags,

    // WebAssembly specific configuration
    pub wasm_config: WasmConfig<'a>,

    // JavaScript specific configuration
    pub js_format: Option<JsFormat>,

    // Extra options
    pub extra_link_opts: &'a [&'a str],
}

/// WebAssembly-specific linking configuration
#[derive(Debug, Default)]
pub struct WasmConfig<'a> {
    pub exports: Option<&'a [String]>,
    pub export_memory_name: Option<Cow<'a, str>>,
    pub import_memory: Option<&'a ImportMemory>,
    pub memory_limits: Option<&'a MemoryLimits>,
    pub shared_memory: Option<bool>,
    pub heap_start_address: Option<u32>,
    pub link_flags: Option<&'a [String]>,
}

impl<'a> MooncLinkCore<'a> {
    /// Create a new instance with only necessary fields populated, others as default
    pub fn new(
        core_deps: &'a [PathBuf],
        main_package: &'a PackageFQN,
        output_path: impl Into<Cow<'a, Path>>,
        pkg_config_path: impl Into<Cow<'a, Path>>,
        package_sources: &'a [PackageSource<'a>],
        target_backend: TargetBackend,
    ) -> Self {
        Self {
            core_deps,
            main_package,
            output_path: output_path.into(),
            pkg_config_path: pkg_config_path.into(),
            package_sources,
            stdlib_core_source: None,
            target_backend,
            flags: CompilationFlags {
                no_opt: false,
                symbols: false,
                source_map: false,
                enable_coverage: false,
                self_coverage: false,
                enable_value_tracing: false,
            },
            wasm_config: WasmConfig::default(),
            js_format: None,
            extra_link_opts: &[],
        }
    }

    /// Convert this to list of args. The behavior tries to mimic the legacy
    /// behavior as much as possible, maintaining EXACT argument order.
    pub fn to_args_legacy(&self, args: &mut Vec<String>) {
        // Command name
        args.push("link-core".into());

        // Core dependencies (input files) - first in legacy order
        for core_dep in self.core_deps {
            args.push(core_dep.to_string_lossy().into_owned());
        }

        // Main package specification
        args.push("-main".to_string());
        args.push(self.main_package.to_string());

        // Output file
        args.push("-o".to_string());
        args.push(self.output_path.display().to_string());

        // Package configuration path
        args.push("-pkg-config-path".to_string());
        args.push(self.pkg_config_path.display().to_string());

        // Package sources (using existing to_arg method)
        for pkg_source in self.package_sources {
            args.push("-pkg-sources".to_string());
            args.push(pkg_source.to_arg());
        }

        // Standard library (if not no_std)
        if let Some(ref stdlib_core) = self.stdlib_core_source {
            args.push("-pkg-sources".to_string());
            args.push(stdlib_core.to_arg());
        }

        // Target backend
        args.push("-target".to_string());
        args.push(self.target_backend.to_flag().to_string());

        // Debug and optimization flags (using CompilationFlags)
        // symbols maps to -g, no_opt maps to -O0
        if self.flags.symbols {
            args.push("-g".to_string());
        }
        if self.flags.no_opt {
            args.push("-O0".to_string());
        }

        // Source map
        if self.flags.source_map {
            args.push("-source-map".to_string());
        }

        // WASM-specific config
        if matches!(
            self.target_backend,
            TargetBackend::Wasm | TargetBackend::WasmGC
        ) {
            // WebAssembly exports
            if let Some(exports) = self.wasm_config.exports {
                if exports.is_empty() {
                    // Empty exports case - legacy adds empty string
                    args.push("".to_string());
                } else {
                    args.push(format!("-exported_functions={}", exports.join(",")));
                }
            }

            // Export memory name
            if let Some(export_memory_name) = &self.wasm_config.export_memory_name {
                args.push("-export-memory-name".to_string());
                args.push(export_memory_name.to_string());
            }

            // Import memory
            if let Some(import_memory) = self.wasm_config.import_memory {
                args.push("-import-memory-module".to_string());
                args.push(import_memory.module.clone());
                args.push("-import-memory-name".to_string());
                args.push(import_memory.name.clone());
            }

            // Memory limits
            if let Some(memory_limits) = self.wasm_config.memory_limits {
                args.push("-memory-limits-min".to_string());
                args.push(memory_limits.min.to_string());
                args.push("-memory-limits-max".to_string());
                args.push(memory_limits.max.to_string());
            }

            // Shared memory
            if let Some(shared_memory) = self.wasm_config.shared_memory {
                if shared_memory {
                    args.push("-shared-memory".to_string());
                }
            }

            // Heap start address
            if let Some(heap_start_address) = self.wasm_config.heap_start_address {
                args.push("-heap-start-address".to_string());
                args.push(heap_start_address.to_string());
            }

            // Link flags
            if let Some(link_flags) = self.wasm_config.link_flags {
                for flag in link_flags {
                    args.push(flag.clone());
                }
            }
        }

        // JavaScript format (only for JS target)
        if self.target_backend == TargetBackend::Js {
            if let Some(js_format) = self.js_format {
                args.push("-js-format".to_string());
                args.push(js_format.to_flag().to_string());
            } else {
                // Use default JS format
                args.push("-js-format".to_string());
                args.push(JsFormat::default().to_flag().to_string());
            }
        }

        // Extra link options
        for opt in self.extra_link_opts {
            args.push(opt.to_string());
        }

        // Windows-specific LLVM target workaround (conditional compilation like legacy)
        // FIXME: We should always provide target info for LLVM
        #[cfg(target_os = "windows")]
        if self.target_backend == TargetBackend::LLVM {
            use moonutil::compiler_flags::CC;
            if CC::default().is_msvc() {
                args.push("-llvm-target".to_string());
                args.push("x86_64-pc-windows-msvc".to_string());
            }
        }
    }
}

impl CmdlineAbstraction for MooncLinkCore<'_> {
    fn to_args(&self, args: &mut Vec<String>) {
        self.to_args_legacy(args);
    }
}
