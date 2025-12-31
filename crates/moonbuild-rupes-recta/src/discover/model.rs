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

//! Data models for discovered packages

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use moonbuild::expect::PackageSrcResolver;
use moonutil::common::{MOON_PKG, MOON_PKG_JSON, TargetBackend};
use moonutil::mooncakes::{ModuleId, ModuleSource};
use moonutil::package::MoonPkg;
use slotmap::{SecondaryMap, SlotMap};

use crate::{
    model::PackageId,
    pkg_name::{PackageFQN, PackageFQNWithSource, PackagePath},
};

// Be careful adding more fields to this struct. If it's not needed everywhere,
// consider calculating it on-demand instead of storing it.
#[derive(Debug, Clone)]
pub struct DiscoveredPackage {
    /// The folder of the package in source tree
    pub root_path: PathBuf,
    /// The ID of the module this package is in
    pub module: ModuleId,
    /// The fully-qualified name of the package
    pub fqn: PackageFQN,

    /// Whether this is a synthetic single-file package
    ///
    /// Single-file packages behave differently in certain aspects, such as
    /// file determination and import resolution.
    pub is_single_file: bool,

    /// The raw `moon.pkg.json` of this package.
    pub raw: Box<MoonPkg>,

    /// `.mbt` files contained by this package. This list contains absolute
    /// paths of the files. The same applies to all other file lists below.
    ///
    /// This is an **unfiltered** list of source files contained by this
    /// package, which requires further classifying into e.g. source files, test
    /// files, and platform-specific files.
    pub source_files: Vec<PathBuf>,

    /// MoonBit Lex files (`.mbl`) contained by this package.
    ///
    /// TODO: Most of these logic are replaced with pre-build tasks, and
    /// `moonlex`/`moonyacc` bundled with the toolchain is not updated
    /// frequently. Consider deprecating these fields and related logic.
    pub mbt_lex_files: Vec<PathBuf>,
    /// MoonBit Yacc files (`.mby`) contained by this package.
    pub mbt_yacc_files: Vec<PathBuf>,
    /// Documentation-oriented programming Markdown files (`.mbt.md`) contained
    /// by this package.
    pub mbt_md_files: Vec<PathBuf>,
    /// C stub files (`.c`) contained by this package. Note that this file list
    /// is generated from the package json, instead of directly collected from
    /// the folder.
    pub c_stub_files: Vec<PathBuf>,

    /// The text-format module interface file for virtual packages.
    ///
    /// This is `None` for non-virtual packages.
    pub virtual_mbti: Option<PathBuf>,

    /// Whether this package is part of the standard library.    
    pub is_stdlib: bool,
}

impl DiscoveredPackage {
    /// Get the configuration file `moon.pkg.json` or `moon.pkg` of this package
    ///
    /// This function assumes regular project layout.
    /// Prefers `moon.pkg` (DSL format) if it exists, otherwise falls back to `moon.pkg.json`.
    pub fn config_path(&self) -> PathBuf {
        if self.root_path.join(MOON_PKG).exists() {
            self.root_path.join(MOON_PKG)
        } else {
            // Default to JSON format (for backward compatibility and single-file scenarios)
            self.root_path.join(MOON_PKG_JSON)
        }
    }

    /// Get whether if the package is a virtual package
    pub fn is_virtual(&self) -> bool {
        self.raw.virtual_pkg.is_some()
    }

    /// Get whether this is an implementation of a virtual package
    pub fn is_virtual_impl(&self) -> bool {
        self.raw.implement.is_some()
    }

    /// Get whether if the package has a concrete implementation, i.e. moonbit
    /// code to compile.
    ///
    /// This include both a regular package and a virtual package with a default
    /// implementation.
    pub fn has_implementation(&self) -> bool {
        self.raw.virtual_pkg.is_none()
            || self.raw.virtual_pkg.as_ref().is_some_and(|x| x.has_default)
    }

    pub fn exported_functions(&self, backend: TargetBackend) -> Option<&[String]> {
        match backend {
            TargetBackend::Wasm => self.raw.link.as_ref()?.wasm.as_ref()?.exports.as_deref(),
            TargetBackend::WasmGC => self.raw.link.as_ref()?.wasm_gc.as_ref()?.exports.as_deref(),
            TargetBackend::Js => self.raw.link.as_ref()?.js.as_ref()?.exports.as_deref(),
            TargetBackend::Native | TargetBackend::LLVM => {
                self.raw.link.as_ref()?.native.as_ref()?.exports.as_deref()
            }
        }
    }
}

/// The result of a package discovery process.
#[derive(Debug, Clone, Default)]
pub struct DiscoverResult {
    /// The directory of all discovered packages
    packages: SlotMap<PackageId, DiscoveredPackage>,

    /// The index from modules to the packages they contain
    module_map: SecondaryMap<ModuleId, BTreeMap<PackagePath, PackageId>>,

    /// Reverse map from package FQN string to package ID
    ///
    /// Currently, we assume that packages names should be unique across all
    /// dependencies. If we allow incompatible versions of the same module
    /// later, this map will not work, and a per-module package name map should
    /// be used instead.
    packages_rev_map: HashMap<String, PackageId>,

    /// A special case: `moonbitlang/core/abort`, a standard library package that
    /// needs special treatments.
    abort_pkg: Option<PackageId>,
}

impl DiscoverResult {
    /// Add a discovered package to the result.
    ///
    /// If a package with the same fully-qualified name already exists, an error
    /// is returned.
    pub(super) fn add_package(
        &mut self,
        m: ModuleId,
        path: PackagePath,
        data: DiscoveredPackage,
    ) -> Result<PackageId, DiscoverError> {
        let id = self.packages.insert(data);
        self.module_map
            .entry(m)
            .expect("There should not be replacement in this map")
            .or_default()
            .insert(path, id);

        if let Some(original) = self
            .packages_rev_map
            .insert(self.packages[id].fqn.to_string(), id)
        {
            return Err(DiscoverError::ConflictingPackageNameString {
                first: self.packages[original].fqn.clone().into(),
                second: self.packages[id].fqn.clone().into(),
            });
        }

        Ok(id)
    }

    pub(super) fn set_abort_pkg(&mut self, id: PackageId) {
        self.abort_pkg = Some(id);
    }

    /// Get a package by its ID. This operation is infallible because PackageId
    /// is only created by this struct.
    pub fn get_package(&self, id: PackageId) -> &DiscoveredPackage {
        &self.packages[id]
    }

    /// Get a mutable handle to a package by its ID.
    pub fn get_package_mut(&mut self, id: PackageId) -> &mut DiscoveredPackage {
        &mut self.packages[id]
    }

    /// Get the package ID for a given module and package path.
    pub fn get_package_id(&self, module: ModuleId, path: &PackagePath) -> Option<PackageId> {
        self.module_map.get(module)?.get(path).copied()
    }

    /// Get the package ID by its fully-qualified name string.
    pub fn get_package_id_by_name(&self, name: &str) -> Option<PackageId> {
        self.packages_rev_map.get(name).copied()
    }

    /// Get all packages for a given module.
    pub fn packages_for_module(
        &self,
        module: ModuleId,
    ) -> Option<&BTreeMap<PackagePath, PackageId>> {
        self.module_map.get(module)
    }

    /// Get all discovered packages.
    pub fn all_packages(
        &self,
        exclude_stdlib: bool,
    ) -> impl Iterator<Item = (PackageId, &DiscoveredPackage)> {
        self.packages
            .iter()
            .filter(move |(_, pkg)| !exclude_stdlib || !pkg.is_stdlib)
    }

    /// Get the number of discovered packages.
    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// Get the FQN of a package by its ID.
    pub fn fqn(&self, id: PackageId) -> PackageFQN {
        let pkg = &self.packages[id];
        pkg.fqn.clone()
    }

    pub fn abort_pkg(&self) -> Option<PackageId> {
        self.abort_pkg
    }

    pub fn is_stdlib_package(&self, id: PackageId) -> bool {
        self.packages[id].is_stdlib
    }
}

impl PackageSrcResolver for DiscoverResult {
    fn resolve_pkg_src(&self, pkg_path: &str) -> PathBuf {
        let pkg_id = self.packages_rev_map[pkg_path];
        self.packages[pkg_id].root_path.clone()
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DiscoverError {
    #[error(
        "Unable to read `moon.mod.json` for module '{module}' at path '{path}', error: {inner}"
    )]
    CantReadModuleFile {
        module: ModuleSource,
        path: PathBuf,
        inner: anyhow::Error,
    },

    #[error("Module name mismatch when reading '{read}', the name in registry is '{registry}'")]
    ModuleNameMismatch { registry: String, read: String },

    #[error("Unable to fetch info for packages in module '{module}', error: {inner}")]
    CantReadModulePackages {
        module: ModuleSource,
        inner: anyhow::Error,
    },

    #[error(
        "Unable to read `moon.pkg.json` for module '{module}' package '{package}' \
        at path '{path}', error: {inner}"
    )]
    CantReadPackageFile {
        module: ModuleSource,
        package: PackagePath,
        path: PathBuf,
        inner: anyhow::Error,
    },

    #[error(
        "Unable to list directory contents for package '{package}' in module '{module}' at path '{path}', error: {inner}"
    )]
    CantListPackageDir {
        module: ModuleSource,
        package: PackagePath,
        path: PathBuf,
        inner: anyhow::Error,
    },

    #[error(
        "Unable to read file info for file '{file}' in package '{package}' of module '{module}', error: {inner}"
    )]
    CantReadFileInfo {
        module: ModuleSource,
        package: PackagePath,
        file: PathBuf,
        inner: anyhow::Error,
    },

    #[error(
        "C stub file path '{path}' in package '{package}' of module '{module}' is invalid: {msg}"
    )]
    InvalidStubPath {
        module: ModuleSource,
        package: PackagePath,
        path: String,
        msg: &'static str,
    },

    #[error("Cannot find `pkg.mbti` declaration file for virtual package {0}")]
    MissingVirtualMbtiFile(PackageFQNWithSource),

    #[error("Duplicated package name `{}` used by both packages {first} from {} and {second} from {}", .first.fqn(), .first.fqn().module(), .second.fqn().module())]
    ConflictingPackageNameString {
        first: PackageFQNWithSource,
        second: PackageFQNWithSource,
    },
}
