//! Compiler command abstraction

use std::path::Path;

use moonutil::common::TargetBackend;

use crate::pkg_name::PackageFQN;

#[allow(unused)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorFormat {
    Regular,
    Json,
}

#[derive(Clone, Debug)]
pub struct MiDependency<'a> {
    pub name: &'a Path,
    pub alias: &'a str,
}

impl<'a> MiDependency<'a> {
    pub fn to_alias_arg(&self) -> String {
        format!("{}:{}", self.name.display(), self.alias)
    }
}

#[derive(Clone, Debug)]
pub struct PackageSource<'a> {
    pub package_name: &'a PackageFQN,
    pub source_dir: &'a Path,
}

impl<'a> PackageSource<'a> {
    pub fn to_arg(&self) -> String {
        format!("{}:{}", self.package_name, self.source_dir.display())
    }
}

#[derive(Clone, Debug)]
pub struct VirtualPackageImplementation<'a> {
    pub mi_path: &'a Path,
    pub package_name: &'a PackageFQN,
    pub package_path: &'a Path,
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
#[derive(Clone, Debug)]
pub enum WarnAlertConfig<'a> {
    Default,
    List(&'a str),
    DenyAll,
    AllowAll,
}

/// Abstraction for `moonc build-package`.
///
/// FIXME: This is a shallow abstraction that tries to mimic the legacy
/// behavior as much as possible. It definitely contains some suboptimal
/// abstractions, which needs to be fixed in the future.
#[derive(Debug)]
pub struct MooncBuildPackage<'a> {
    // Basic command structure
    pub error_format: ErrorFormat,

    // Warning and alert configuration
    pub warn_config: WarnAlertConfig<'a>,
    pub alert_config: WarnAlertConfig<'a>,

    // Input files
    pub mbt_sources: &'a [&'a Path],
    pub mi_deps: &'a [MiDependency<'a>],

    // Output configuration
    pub core_out: &'a Path,
    pub mi_out: &'a Path,
    pub no_mi: bool,

    // Package configuration
    /// The name of the current package
    pub package_name: &'a PackageFQN,
    /// The source directory of the current package
    pub package_source: &'a Path,
    pub is_main: bool,

    // Standard library
    /// Pass [None] for no_std
    pub stdlib_core_file: Option<&'a Path>,

    // Target configuration
    pub target_backend: TargetBackend,

    // Compilation flags
    pub flags: CompilationFlags,

    // Extra options
    pub extra_build_opts: &'a [&'a str],

    // Virtual package handling
    // FIXME: better abstraction
    pub check_mi: Option<&'a Path>,
    pub virtual_implementation: Option<VirtualPackageImplementation<'a>>,
}

impl<'a> MooncBuildPackage<'a> {
    /// Convert this to list of args. The behavior tries to mimic the legacy
    /// behavior as much as possible.
    pub fn to_args_legacy(&self) -> Vec<String> {
        let mut args = vec!["build-package".to_string()];

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
        if let WarnAlertConfig::List(warn_list) = self.warn_config {
            args.extend(["-w".to_string(), warn_list.to_string()]);
        }
        if let WarnAlertConfig::List(alert_list) = self.alert_config {
            args.extend(["-alert".to_string(), alert_list.to_string()]);
        }
        // Third-party package handling
        if matches!(self.warn_config, WarnAlertConfig::AllowAll) {
            args.extend(["-w".to_string(), "-a".to_string()]);
        }
        if matches!(self.alert_config, WarnAlertConfig::AllowAll) {
            args.extend(["-alert".to_string(), "-all".to_string()]);
        }

        // Output
        args.extend(["-o".to_string(), self.core_out.display().to_string()]);

        // Package configuration
        args.extend(["-pkg".to_string(), self.package_name.to_string()]);

        if self.is_main {
            args.push("-is-main".to_string());
        }

        // Standard library
        if let Some(stdlib_path) = self.stdlib_core_file {
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
        if let Some(check_mi_path) = self.check_mi {
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

        args
    }
}

const MOONC_REGULAR_WARNING_SET: &str = "+a-31-32";
#[allow(unused)]
const MOONC_REGULAR_ALERT_SET: &str = "+all-raise-throw-unsafe+deprecated";

const MOONC_DENY_WARNING_SET: &str = "@a-31-32";
const MOONC_DENY_ALERT_SET: &str = "@all-raise-throw-unsafe+deprecated";
const MOONC_ALLOW_WARNING_SET: &str = "-a";
const MOONC_ALLOW_ALERT_SET: &str = "-all";
