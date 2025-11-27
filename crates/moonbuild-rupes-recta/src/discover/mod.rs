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

//! Discovers packages and build targets from source directory layouts.
//!
//! The discover process is designed to be minimal, only fetching required
//! information from the file system. Later stages that do not require file
//! system access should be split into a separate module instead of coupled with
//! this discover process.

// Specifically allow file I/O here, because that what this module is about.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

mod special_case;
pub mod synth;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use log::{debug, info, trace};
use moonutil::mooncakes::{DirSyncResult, ModuleId, ModuleSource, result::ResolvedEnv};
use moonutil::package::MoonPkg;
use moonutil::{
    common::{
        IGNORE_DIRS, MBTI_USER_WRITTEN, MOON_MOD_JSON, MOON_PKG_JSON, TargetBackend,
        read_module_desc_file_in_dir, read_package_desc_file_in_dir,
    },
    mooncakes::ModuleSourceKind,
};
use relative_path::{PathExt, RelativePath};
use slotmap::{SecondaryMap, SlotMap};
use tracing::{Level, instrument, warn};
use walkdir::WalkDir;

use crate::{
    discover::special_case::inject_std_abort,
    model::PackageId,
    pkg_name::{PackageFQN, PackageFQNWithSource, PackagePath},
    special_cases::{add_prelude_as_import_for_core, module_name_is_core},
    util::strip_trailing_slash,
};

/// Discover packages contained by all dependencies from their paths
#[instrument(skip_all)]
pub fn discover_packages(
    env: &ResolvedEnv,
    dirs: &DirSyncResult,
) -> Result<DiscoverResult, DiscoverError> {
    info!("Starting package discovery across all modules");
    let mut res = DiscoverResult::default();

    debug!("Discovering packages in {} modules", env.module_count());

    for (id, m) in env.all_modules_and_id() {
        // SPECIAL_CASE: Skip stdlib in discovering. They are handled below.
        if let ModuleSourceKind::Stdlib(_) | ModuleSourceKind::SingleFile(_) = m.source() {
            continue;
        };

        let dir = dirs.get(id).expect("Bad module ID to get directory");
        discover_packages_for_mod(&mut res, env, dir, id, m)?;
    }

    // Inject `moonbitlang/core/abort` package to be used by other packages
    inject_std_abort(env, dirs, &mut res)?;

    info!(
        "Package discovery completed: found {} packages across {} modules",
        res.package_count(),
        env.module_count()
    );

    Ok(res)
}

/// Discover packages within the given module directory
#[instrument(level = Level::DEBUG, skip(res, env, dir, module_source))]
pub(crate) fn discover_packages_for_mod(
    res: &mut DiscoverResult,
    env: &ResolvedEnv,
    dir: &Path,
    id: ModuleId,
    module_source: &ModuleSource,
) -> Result<(), DiscoverError> {
    // This information is the one we get from the registry. We will read again
    // from the resolved directory
    let m_registry = env.module_info(id);

    info!(
        "Begin discovering packages for {} at {}",
        module_source,
        dir.display()
    );

    // This is the version we read from directory
    let m = read_module_desc_file_in_dir(dir).map_err(|e| DiscoverError::CantReadModuleFile {
        module: module_source.clone(),
        path: dir.to_owned(),
        inner: e,
    })?;

    // Do some basic sanity checks
    if m.name != m_registry.name {
        return Err(DiscoverError::ModuleNameMismatch {
            registry: m_registry.name.clone(),
            read: m.name.clone(),
        });
    }

    let source_dir_name = m.source.as_deref().unwrap_or("");
    let scan_source_root = {
        let p = dir.join(source_dir_name);
        dunce::canonicalize(p).map_err(|e| DiscoverError::CantReadModulePackages {
            module: module_source.clone(),
            inner: e.into(),
        })?
    };
    let is_core = module_name_is_core(&m.name);

    // Recursively walk through the module's directories
    let mut walkdir = WalkDir::new(&scan_source_root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|x| x.file_type().is_dir());
    while let Some(entry) = walkdir.next() {
        let entry = entry.map_err(|e| DiscoverError::CantReadModulePackages {
            module: module_source.clone(),
            inner: e.into(),
        })?;

        let abs_path = strip_trailing_slash(entry.path());
        // this will be fed to package path
        let rel_path = abs_path
            .relative_to(&scan_source_root)
            .expect("Walked directory should be a descendant of the scan source");

        // Skip certain ignored directories
        if let Some(filename) = rel_path.file_name()
            && IGNORE_DIRS.contains(&filename)
        {
            debug!(
                "Skipping {} recursively because it is in the internal ignored list",
                abs_path.display()
            );
            walkdir.skip_current_dir();
            continue;
        }

        // Avoid descending into another module
        let mod_json_path = abs_path.join(MOON_MOD_JSON);
        if mod_json_path.exists() && rel_path != "" {
            debug!(
                "Skipping {} recursively because it contains {}",
                abs_path.display(),
                MOON_MOD_JSON
            );
            walkdir.skip_current_dir();
            continue;
        }

        // Check if this directory is a package
        let pkg_json_path = abs_path.join(MOON_PKG_JSON);
        if !pkg_json_path.exists() {
            debug!(
                "Skipping {} because it does not contain {}",
                abs_path.display(),
                MOON_PKG_JSON
            );
            continue;
        }

        // Begin discovering the package
        debug!("Discovering package at {}", abs_path.display());
        let pkg = discover_one_package(id, module_source, abs_path, &rel_path, is_core)?;
        debug!(
            "Found package: {} with {} source files",
            pkg.fqn,
            pkg.source_files.len()
        );
        res.add_package(id, pkg.fqn.package().clone(), pkg);
    }

    Ok(())
}

/// Discover one package and get its basic information. This does *not* create
/// e.g. subpackages.
#[instrument(level = Level::DEBUG, skip(m, abs, rel))]
fn discover_one_package(
    mid: ModuleId,
    m: &ModuleSource,
    abs: &Path,
    rel: &RelativePath,
    is_core: bool, // We have a couple of special cases for core packages
) -> Result<DiscoveredPackage, DiscoverError> {
    let pkg_path = PackagePath::new_from_rel_path(rel)
        .expect("Generation of package path from relative path should not error");
    let fqn = PackageFQN::new(m.clone(), pkg_path);

    // Discover the package config
    let pkg_json =
        read_package_desc_file_in_dir(abs).map_err(|e| DiscoverError::CantReadPackageFile {
            module: m.clone(),
            package: fqn.package().clone(),
            path: abs.to_path_buf(),
            inner: e,
        })?;
    let pkg_json = if is_core {
        add_prelude_as_import_for_core(pkg_json)
    } else {
        pkg_json
    };

    // Discover source files within the package
    let mut source_files = Vec::new();
    let mut mbt_lex_files = Vec::new();
    let mut mbt_yacc_files = Vec::new();
    let mut mbt_md_files = Vec::new();

    let dir = abs
        .read_dir()
        .map_err(|x| DiscoverError::CantListPackageDir {
            module: m.clone(),
            package: fqn.package().clone(),
            path: abs.to_owned(),
            inner: x.into(),
        })?;
    for file in dir {
        let file = file.map_err(|e| DiscoverError::CantListPackageDir {
            module: m.clone(),
            package: fqn.package().clone(),
            path: abs.to_owned(),
            inner: e.into(),
        })?;
        let path = file.path();
        let file_info = file
            .metadata()
            .map_err(|e| DiscoverError::CantReadFileInfo {
                module: m.clone(),
                package: fqn.package().clone(),
                file: path.clone(),
                inner: e.into(),
            })?;

        if !file_info.is_file() {
            // Only files are included within the package
            continue;
        }
        trace!("Found file {}", path.display());

        let filename = path
            .file_name()
            .expect("We are listing a dir, file should have name");
        let filename_str = filename.to_string_lossy();
        if filename_str.ends_with(".mbt") {
            source_files.push(path)
        } else if filename_str.ends_with(".mbt.md") {
            mbt_md_files.push(path);
        } else if filename_str.ends_with(".mbl") {
            mbt_lex_files.push(path);
        } else if filename_str.ends_with(".mby") {
            mbt_yacc_files.push(path);
        } else {
            // File is not one of our expected types, skip
        }
    }

    // Read C stubs from package json
    let mut c_stubs = Vec::new();
    if let Some(stub_list) = &pkg_json.native_stub {
        for stub in stub_list {
            let rel_path = RelativePath::new(&stub).normalize();
            // Check if path is valid
            if rel_path.starts_with("..") {
                return Err(DiscoverError::InvalidStubPath {
                    module: m.clone(),
                    package: fqn.package().clone(),
                    path: stub.clone(),
                    msg: "Path descends into parent directory",
                });
            }
            c_stubs.push(rel_path.to_path(abs));
        }
    };

    // Sort the source files for repeatable results
    let _sort_guard = tracing::debug_span!("sorting_files").entered();
    source_files.sort();
    mbt_lex_files.sort();
    mbt_yacc_files.sort();
    mbt_md_files.sort();
    drop(_sort_guard);

    // Get the virtual mbti file if any
    let virtual_mbti = discover_virtual_mbti(&pkg_json, &fqn, abs)?;

    Ok(DiscoveredPackage {
        root_path: abs.to_path_buf(),
        module: mid,
        fqn,
        is_single_file: false,
        raw: Box::new(pkg_json),
        source_files,
        mbt_lex_files,
        mbt_yacc_files,
        mbt_md_files,
        c_stub_files: c_stubs,
        virtual_mbti,
    })
}

fn discover_virtual_mbti(
    pkg_json: &MoonPkg,
    fqn: &PackageFQN,
    abs: &Path,
) -> Result<Option<PathBuf>, DiscoverError> {
    let res = if pkg_json.virtual_pkg.is_some() {
        // There are two types of `.mbti` files accepted as input:
        // - The newer version is `pkg.mbti`
        // - The older version is `<pkg_short_name>.mbti`
        // We prefer the newer one if possible.
        let short_name = fqn.short_alias();

        let new_mbti = abs.join(MBTI_USER_WRITTEN);
        let has_new_mbti = new_mbti.exists();
        let old_mbti = abs.join(format!("{}.mbti", short_name));
        let has_old_mbti = old_mbti.exists();

        if has_new_mbti {
            Some(new_mbti)
        } else if has_old_mbti {
            warn!(
                "Using package name in MBTI file is deprecated. Please rename {} to {}",
                old_mbti.display(),
                MBTI_USER_WRITTEN
            );
            Some(old_mbti)
        } else {
            return Err(DiscoverError::MissingVirtualMbtiFile(fqn.clone().into()));
        }
    } else {
        None
    };

    Ok(res)
}

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
    /// TODO: These files should not be handled by the build system.
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
}

impl DiscoveredPackage {
    /// Get the configuration file `moon.pkg.json` of this package
    ///
    /// This function assumes regular project layout.
    pub fn config_path(&self) -> PathBuf {
        self.root_path.join(MOON_PKG_JSON)
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

    /// A special case: `moonbitlang/core/abort`, a standard library package that
    /// needs special treatments.
    abort_pkg: Option<PackageId>,
}

impl DiscoverResult {
    fn add_package(
        &mut self,
        m: ModuleId,
        path: PackagePath,
        data: DiscoveredPackage,
    ) -> PackageId {
        let id = self.packages.insert(data);
        self.module_map
            .entry(m)
            .expect("There should not be replacement in this map")
            .or_default()
            .insert(path, id);
        id
    }

    /// Get a package by its ID. This operation is infallible because PackageId
    /// is only created by this struct.
    pub fn get_package(&self, id: PackageId) -> &DiscoveredPackage {
        &self.packages[id]
    }

    /// Get the package ID for a given module and package path.
    pub fn get_package_id(&self, module: ModuleId, path: &PackagePath) -> Option<PackageId> {
        self.module_map.get(module)?.get(path).copied()
    }

    /// Get all packages for a given module.
    pub fn packages_for_module(
        &self,
        module: ModuleId,
    ) -> Option<&BTreeMap<PackagePath, PackageId>> {
        self.module_map.get(module)
    }

    /// Get all discovered packages.
    pub fn all_packages(&self) -> impl Iterator<Item = (PackageId, &DiscoveredPackage)> {
        self.packages.iter()
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
}
