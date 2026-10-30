// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Data models for discovered packages

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

use moonutil::manifest::{MoonMod, MoonModRule};
use moonutil::package::{MoonPkg, SupportedTargetsDeclKind};
use moonutil::resolution::{ModuleId, ModuleSource, ResolvedRootModules};
use moonutil::{constants::MOON_PKG_JSON, target::TargetBackend};
use slotmap::{SecondaryMap, SlotMap};

use crate::{
    model::PackageId,
    pkg_name::{PackageFQN, PackageFQNWithSource, PackagePath},
};

/// Construct a declared prebuild file path in the package file identity.
///
/// Discovery, formatting, and build planning share this constructor so that
/// each compares complete paths without reconciling different spellings.
pub(crate) fn resolve_prebuild_path(package_root: &Path, path: &str) -> PathBuf {
    // Construct declared paths in the same form as discovered package files,
    // before they enter the plan. Outputs may not exist yet, so resolve their
    // components lexically against the canonical package root.
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};

        let path = package_root.join(path);
        let mut components = path.components();
        let Some(Component::Prefix(prefix)) = components.next() else {
            return path.components().collect();
        };
        let root = match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) if path.is_absolute() => {
                // Rust parses drive letters uppercase. Rebuild verbatim drives
                // too: n2 compares raw strings, unlike PathBuf's drive equality.
                PathBuf::from(format!(r"\\?\{}:", char::from(drive)))
            }
            Prefix::UNC(server, share) => {
                // A UNC prefix must be complete before native joining parses it.
                let mut root = std::ffi::OsString::from(r"\\?\UNC\");
                root.push(server);
                root.push(r"\");
                root.push(share);
                PathBuf::from(root)
            }
            _ => PathBuf::from(prefix.as_os_str()),
        };
        // An absolute declaration may have replaced the package root with an
        // ordinary drive/UNC root. Restore its verbatim spelling, then let
        // native joining resolve separators and parent components.
        root.join(components.as_path())
    }
    #[cfg(not(windows))]
    {
        let root = if Path::new(path).is_absolute() {
            Path::new("/")
        } else {
            package_root
        };
        relative_path::RelativePath::new(path)
            .to_logical_path(root)
            .components()
            .collect()
    }
}

/// The source format of a synthetic single-file package.
///
/// This is determined from the invoked filename when resolving the single-file
/// input. Later build stages consume this interpretation instead of inferring
/// semantics from the canonical file identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SingleFileSourceKind {
    Mbt,
    MbtMd,
    Mbtx,
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

    /// The source format when this is a synthetic single-file package.
    ///
    /// Single-file packages behave differently in certain aspects, such as
    /// file determination and import resolution.
    pub single_file_source_kind: Option<SingleFileSourceKind>,

    /// The selected package manifest path for normal package layouts.
    ///
    /// Synthetic single-file packages do not have a real package manifest.
    pub manifest_path: Option<PathBuf>,

    /// The raw `moon.pkg.json` of this package.
    pub raw: Box<MoonPkg>,
    /// How backend `supported_targets` was declared in package config.
    pub supported_targets_decl: SupportedTargetsDeclKind,
    /// Effective backend support used by build planning and backend checks.
    ///
    /// This is the package-supported targets intersected with the containing
    /// module's supported targets.
    pub effective_supported_targets: indexmap::IndexSet<TargetBackend>,

    /// Source inputs awaiting Build Target Projection.
    ///
    /// For an ordinary package, this is its unfiltered set of discovered
    /// `.mbt` files. Single-file mode instead carries its selected `.mbt` or
    /// `.mbtx` input here; `.mbtx` is never discovered as part of an ordinary
    /// package. Paths in this and the other file lists below are absolute.
    pub source_files: Vec<PathBuf>,

    /// Documentation-oriented programming Markdown files (`.mbt.md`) contained
    /// by this package.
    pub mbt_md_files: Vec<PathBuf>,
    /// Verification helper files (`.mbtp`) contained by this package.
    pub mbtp_files: Vec<PathBuf>,
    /// C stub files (`.c`) contained by this package. Note that this file list
    /// is generated from the package json, instead of directly collected from
    /// the folder.
    pub c_stub_files: Vec<PathBuf>,
    /// Package-local header files that may be observed while compiling C
    /// stubs.
    ///
    /// Every C-stub action conservatively depends on this complete set because
    /// MoonBuild does not interpret transitive `#include` directives.
    pub c_stub_header_files: Vec<PathBuf>,

    /// Existing text-format module interface files for virtual packages.
    ///
    /// These are only paths observed during Package File Set discovery. Build
    /// planning combines them with actual package prebuild outputs and selects
    /// the input for `BuildVirtual`.
    pub virtual_mbti_files: Vec<PathBuf>,

    /// Whether this package comes from the injected prebuilt standard library.
    ///
    /// Its artifacts are installed toolchain files rather than outputs of the
    /// build plan. Packages of `moonbitlang/core` are not marked when building
    /// core itself, and no package is marked when the stdlib is not injected.
    pub is_stdlib: bool,
}

impl DiscoveredPackage {
    pub fn is_single_file(&self) -> bool {
        self.single_file_source_kind.is_some()
    }

    pub fn is_mbtx_single_file(&self) -> bool {
        self.single_file_source_kind == Some(SingleFileSourceKind::Mbtx)
    }

    /// Get the configuration file `moon.pkg.json` or `moon.pkg` of this package.
    pub fn config_path(&self) -> PathBuf {
        self.manifest_path
            .clone()
            .unwrap_or_else(|| self.root_path.join(MOON_PKG_JSON))
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

    pub fn local_rules(&self) -> Option<&[MoonModRule]> {
        self.raw.local_rules.as_deref()
    }
}

/// The result of a package discovery process.
#[derive(Debug, Clone, Default)]
pub struct DiscoverResult {
    /// Module manifests read from the resolved source directories.
    module_infos: SecondaryMap<ModuleId, Arc<MoonMod>>,

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
    ///
    /// Note: this is purely a discovery-time record. It is set whenever abort
    /// is present, regardless of whether we are building the stdlib itself or
    /// injecting a prebuilt stdlib. Build-mode checks decide whether the
    /// prebuilt abort artifacts are used.
    abort_pkg: Option<PackageId>,
}

/// A local project discovered directly from the file system without dependency resolution.
#[derive(Debug, Clone)]
pub struct DiscoveredLocalProject {
    /// The discovered local root modules.
    pub root_modules: ResolvedRootModules,
    /// The module IDs of the local root modules.
    pub root_module_ids: Vec<ModuleId>,
    /// The packages discovered under the local root modules.
    pub pkg_dirs: DiscoverResult,
}

impl DiscoverResult {
    pub(super) fn set_module_info(&mut self, module: ModuleId, info: Arc<MoonMod>) {
        self.module_infos.insert(module, info);
    }

    pub fn module_info(&self, module: ModuleId) -> &MoonMod {
        self.module_infos
            .get(module)
            .expect("module info should be discovered before use")
            .as_ref()
    }

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

    #[cfg(test)]
    pub(crate) fn test_register_module(&mut self, module: ModuleId, info: MoonMod) {
        self.set_module_info(module, Arc::new(info));
        self.module_map.insert(module, BTreeMap::new());
    }

    #[cfg(test)]
    pub(crate) fn test_add_package(
        &mut self,
        module: ModuleId,
        path: PackagePath,
        data: DiscoveredPackage,
    ) -> PackageId {
        self.add_package(module, path, data)
            .expect("test package should be inserted")
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DiscoverError {
    #[error("Unable to load local workspace rooted at path '{path}', error: {inner}")]
    CantReadLocalWorkspace { path: PathBuf, inner: anyhow::Error },

    #[error("Unable to read local `moon.mod.json` at path '{path}', error: {inner}")]
    CantReadLocalModuleFile { path: PathBuf, inner: anyhow::Error },

    #[error(
        "Unable to read `moon.mod.json` for module '{module}' at path '{path}', error: {inner}"
    )]
    CantReadModuleFile {
        module: ModuleSource,
        path: PathBuf,
        inner: anyhow::Error,
    },

    #[error(
        "Automatic generation from '{file}' is no longer supported. Declare the file in a custom `rule` / `dev_build` (or `pre-build`) task that generates `.mbt` files."
    )]
    UnsupportedLegacyGeneratedSource { file: PathBuf },

    #[error("Module name mismatch when reading '{read}', the name in registry is '{registry}'")]
    ModuleNameMismatch { registry: String, read: String },

    #[error("Unable to fetch info for packages in module '{module}', error: {inner}")]
    CantReadModulePackages {
        module: ModuleSource,
        inner: anyhow::Error,
    },

    #[error(
        "Unable to read `moon.pkg` for module '{module}' package '{package}' \
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

    #[error("Duplicated package name `{}` used by both packages {first} from {} and {second} from {}", .first.fqn(), .first.fqn().module(), .second.fqn().module())]
    ConflictingPackageNameString {
        first: PackageFQNWithSource,
        second: PackageFQNWithSource,
    },

    #[error(
        "Front matter import '{import}' could not be resolved. Make sure its module is listed in moonbit.deps."
    )]
    SingleFileImportNotFound { import: String },
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    #[cfg(windows)]
    use std::path::PathBuf;

    use super::resolve_prebuild_path;

    #[test]
    #[cfg(windows)]
    fn prebuild_paths_use_native_verbatim_joining() {
        for (input_prefix, prefix) in [
            (r"C:", r"\\?\C:"),
            (r"\\?\C:", r"\\?\C:"),
            (r"\\server\share", r"\\?\UNC\server\share"),
            (r"\\?\UNC\server\share", r"\\?\UNC\server\share"),
        ] {
            let root = PathBuf::from(format!(r"{input_prefix}\project\pkg"));
            for (relative, expected) in [
                ("../a/b/c", r"\project\a\b\c"),
                ("../assets/./file", r"\project\assets\file"),
                (r"..\assets\.\file", r"\project\assets\file"),
                (r"../assets\..\file", r"\project\file"),
                ("../../../../file", r"\file"),
                ("../assets/nonexistent", r"\project\assets\nonexistent"),
            ] {
                let path = resolve_prebuild_path(&root, relative);
                assert_eq!(
                    path.as_os_str(),
                    Path::new(&format!("{prefix}{expected}")).as_os_str(),
                    "root={root:?}, relative={relative:?}",
                );
                assert!(!path.to_str().expect("test path is UTF-8").contains('/'));
                assert!(!path.components().any(|component| matches!(
                    component,
                    std::path::Component::CurDir | std::path::Component::ParentDir
                )));
            }
        }
        for (absolute, expected) in [
            (r"D:\assets\..\generated.mbt", r"\\?\D:\generated.mbt"),
            (r"d:\assets\..\generated.mbt", r"\\?\D:\generated.mbt"),
            (r"\\?\d:\assets\..\generated.mbt", r"\\?\D:\generated.mbt"),
            (
                r"\\server\share\assets\..\generated.mbt",
                r"\\?\UNC\server\share\generated.mbt",
            ),
        ] {
            assert_eq!(
                resolve_prebuild_path(Path::new(r"\\?\C:\project\pkg"), absolute).as_os_str(),
                Path::new(expected).as_os_str(),
            );
        }
    }

    #[test]
    fn prebuild_declarations_construct_the_package_file_spelling() {
        let package = if cfg!(windows) {
            Path::new(r"\\?\C:\project\pkg")
        } else {
            Path::new("/project/pkg")
        };
        // Discovery derives file paths from this same package root. No file
        // needs to exist, and no later comparison needs to reconcile spellings.
        let discovered = package.join("generated.mbt");
        let absolute = dunce::simplified(&discovered)
            .to_str()
            .expect("test path is UTF-8");
        for declared in [
            "generated.mbt",
            "./generated.mbt",
            "../pkg/generated.mbt",
            absolute,
        ] {
            assert_eq!(
                resolve_prebuild_path(package, declared).as_os_str(),
                discovered.as_os_str(),
                "declaration={declared:?}",
            );
        }
    }
}
