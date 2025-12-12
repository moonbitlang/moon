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

mod build_common;
mod build_interface;
mod build_package;
mod bundle_core;
mod check;
mod gen_test_driver;
mod link_core;
mod moondoc;
mod mooninfo;

use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::Path;

use crate::model::TargetKind;
use crate::pkg_name::PackageFQN;

pub use self::build_common::*;
pub use self::build_interface::*;
pub use self::build_package::*;
pub use self::bundle_core::*;
pub use self::check::*;
pub use self::gen_test_driver::*;
pub use self::link_core::*;
pub use self::moondoc::*;
pub use self::mooninfo::*;

/// The format of error reports from `moonc`.
///
/// Note that rendering of diagnostics is done in `moon`. `moonc` never directly
/// renders diagnostics to the user.
#[allow(unused)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorFormat {
    /// A semi-structured format with only the location and message.
    Regular,
    /// A fully structured JSON format. `moon` uses this to render diagnostics.
    Json,
}

/// Represents a dependency to the `.mi` (module interface) file of another
/// package.
#[derive(Clone, Debug)]
pub struct MiDependency<'a> {
    /// The path to the `.mi` file of the dependency.
    pub path: Cow<'a, Path>,
    /// An optional alias for the package, to be used when referencing symbols
    /// declared in this package. Also see: [`PackageFQN::short_alias`].
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

/// Represents a package name passed to the compiler. This might add a suffix
/// to the original package name depending on the target kind.
///
/// Note: this is not the same as the filenames used by the artifacts produced
/// by the compiler. This suffix is currently only necessary for blackbox test
/// targets, while the filename need to be deduplicated for every target kind.
#[derive(Clone, Debug)]
pub struct CompiledPackageName<'a> {
    pub fqn: &'a PackageFQN,
    pub kind: TargetKind,
}

impl<'a> CompiledPackageName<'a> {
    pub fn new(fqn: &'a PackageFQN, target_kind: TargetKind) -> Self {
        Self {
            fqn,
            kind: target_kind,
        }
    }
}

impl<'a> std::fmt::Display for CompiledPackageName<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let suffix = match self.kind {
            TargetKind::Source => "",
            // FIXME: `moonc` MANDATES black box tests to have name exactly the
            // original name + "_blackbox_test", in order to support importing
            // all public declaration in the original package. This is an
            // implicit behavior that should be documented and fixed later.
            TargetKind::BlackboxTest => "_blackbox_test",
            // All other target kinds should not have suffixes, or else the
            // tests in `moonbitlang/core` will not have the correct imports.
            TargetKind::WhiteboxTest => "",
            TargetKind::InlineTest => "",
            TargetKind::SubPackage => "",
        };
        write!(f, "{}{}", self.fqn, suffix)
    }
}

/// The mapping from package name to its base directory.
#[derive(Clone, Debug)]
pub struct PackageSource<'a> {
    /// The package name.
    pub package_name: CompiledPackageName<'a>,
    /// The directory containing the package's source files and `moon.pkg.json`.
    pub source_dir: Cow<'a, Path>,
}

impl<'a> PackageSource<'a> {
    pub fn to_arg(&self) -> String {
        format!("{}:{}", self.package_name, self.source_dir.display())
    }
}

/// The information needed to specify an implementation of a virtual package.
///
/// Note: The following data is all about the **virtual package** that is being
/// implemented, not the implementation package itself.
#[derive(Clone, Debug)]
pub struct VirtualPackageImplementation<'a> {
    /// The path to the `.mi` file of the virtual package itself.
    pub mi_path: Cow<'a, Path>,
    /// The name of the virtual package.
    pub package_name: &'a PackageFQN,
    /// The path to the virtual package's source directory.
    pub package_path: Cow<'a, Path>,
}

/// Compilation flags that affect code generation.
#[derive(Clone, Debug)]
pub struct CompilationFlags {
    /// Disable optimization (adds -O0)
    pub no_opt: bool,
    /// Include debug symbols (adds -g)
    pub symbols: bool,
    /// Emit source map file for supported backends (JS and WASM)
    pub source_map: bool,
    /// Enable code coverage instrumentation.
    ///
    /// This injects additional code in the compiled output to track which part
    /// of the code has been executed at runtime. This by default uses the
    /// standard library's code coverage tracking primitives.
    pub enable_coverage: bool,
    /// Use self-coverage mode, which uses the current package itself as the
    /// package implementing code coverage tracking primitives.
    pub self_coverage: bool,
    /// Enable value tracing instrumentation.
    ///
    /// This injects additional code in the compiled output to track the values
    /// of variables at runtime for debugging purposes.
    pub enable_value_tracing: bool,
}

/// Configuration for either warning or alert
#[derive(Clone, Debug, Default)]
#[allow(unused)]
pub enum WarnAlertConfig<'a> {
    /// Use the compiler's default configuration
    #[default]
    Default,
    /// Use a specified list of warnings/alerts that will be passed to the compiler
    List(Cow<'a, str>),
    /// Suppress all warnings/alerts. No optional warnings/alerts will be
    /// reported, only the errors that will prevent successful compilation.
    Suppress,
}

/// The trait for building command line arguments for `moonc` commands.
pub trait CmdlineAbstraction {
    /// Convert this structure to command line arguments.
    fn to_args(&self, args: &mut Vec<String>);

    /// Build the full command with executable and arguments.
    fn build_command(&self, executable: impl AsRef<OsStr>) -> Vec<String> {
        let mut args = vec![executable.as_ref().to_string_lossy().to_string()];
        self.to_args(&mut args);
        args
    }
}

/// The default list of warnings used by `moonc` for regular compilation.
///
/// Only provided as a reference, not actually used in code.
#[allow(unused)]
pub(crate) const MOONC_REGULAR_WARNING_SET: &str = "+a-31-32";
/// The warning list to use when denying all warnings.
pub(crate) const MOONC_DENY_WARNING_SET: &str = "@a";
/// The warning list to use when suppressing all warnings.
pub(crate) const MOONC_SUPPRESS_WARNING_SET: &str = "-a";
