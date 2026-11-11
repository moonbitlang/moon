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

//! Shared filesystem scanning for formatting and build preparation.

use std::path::{Path, PathBuf};

use log::{debug, trace};
use moonutil::{
    constants::{MOON_MOD, MOON_MOD_JSON, MOON_PKG, MOON_PKG_JSON, is_ignored_directory_name},
    manifest::{
        ManifestFormat, MoonMod, preferred_manifest_in_dir,
        read_package_desc_file_from_path_with_supported_targets_decl, warn_known_shadowed_manifest,
    },
    package::{MoonPkg, SupportedTargetsDeclKind},
    resolution::{ModuleName, ModuleSource},
    user_log::UserLog,
};
use relative_path::PathExt;

use super::DiscoverError;
use crate::pkg_name::{PackageFQN, PackagePath};

/// A package declaration and the files observed in its directory. It contains
/// no injected imports or derived build settings. Formatting consumes this
/// directly; build preparation consumes it to construct a DiscoveredPackage.
#[derive(Debug)]
pub(crate) struct PackageInput {
    pub root_path: PathBuf,
    pub fqn: PackageFQN,
    pub manifest_format: ManifestFormat,
    pub manifest: MoonPkg,
    pub supported_targets_decl: SupportedTargetsDeclKind,
    /// Immediate regular files and symlinks, sorted by path. Subdirectories are
    /// traversed separately. Source classification reuses this enumeration.
    pub files: Vec<PathBuf>,
}

pub(crate) fn warn_package_name(package: &PackageFQN, user_log: &UserLog) {
    // TODO: Remove this exception once core/v128 follows the major-version
    // package naming convention. It remains valid when building core itself.
    let core_v128 =
        *package.module().name() == ("moonbitlang", "core") && package.package().as_str() == "v128";
    if !core_v128
        && !package.module().name().username.is_empty()
        && !package.package().is_empty()
        && matches!(
            ModuleName::from(package.to_string().as_str()).major_version_suffix(),
            Ok(Some(_))
        )
    {
        user_log.warn(format!(
            "Package `{}` in module `{}` has the same import path as a major-version module. \
             Consider renaming package `{}` to avoid ambiguity.",
            package,
            package.module().name(),
            package.package(),
        ));
    }
}

/// Scan packages in lexical depth-first order, returning the first read error.
pub(crate) fn scan_packages(
    source: &ModuleSource,
    dir: &Path,
    module: &MoonMod,
    allow_legacy: bool,
    emit_warnings: bool,
    user_log: &UserLog,
) -> Result<Vec<PackageInput>, DiscoverError> {
    // Preserve verbatim Windows prefixes until paths reach their consumers.
    let root = std::fs::canonicalize(dir.join(module.source.as_deref().unwrap_or_default()))
        .map_err(|inner| DiscoverError::CantReadModulePackages {
            module: source.clone(),
            inner: inner.into(),
        })?;
    let mut directories = vec![root.clone()];
    let mut packages = Vec::new();
    while let Some(directory) = directories.pop() {
        let dir = crate::util::strip_trailing_slash(&directory);
        if dir != root && (dir.join(MOON_MOD).exists() || dir.join(MOON_MOD_JSON).exists()) {
            debug!(
                "Skipping {} recursively because it contains a module configuration file",
                dir.display()
            );
            continue;
        }
        let relative = dir
            .relative_to(&root)
            .expect("scanned directory is within module");
        let package = PackagePath::new_from_rel_path(&relative)
            .expect("directory relative to module is a valid package path");

        // One enumeration supplies both child directories and package files.
        let entries =
            std::fs::read_dir(dir).map_err(|inner| DiscoverError::CantReadModulePackages {
                module: source.clone(),
                inner: inner.into(),
            })?;
        let mut children = Vec::new();
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|inner| DiscoverError::CantReadModulePackages {
                module: source.clone(),
                inner: inner.into(),
            })?;
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|inner| DiscoverError::CantReadFileInfo {
                    module: source.clone(),
                    package: package.clone(),
                    file: path.clone(),
                    inner: inner.into(),
                })?;
            if kind.is_dir() {
                // The explicit source root itself is never filtered.
                if !is_ignored_directory_name(&entry.file_name()) {
                    children.push(path);
                }
            } else if kind.is_file() || kind.is_symlink() {
                trace!("Found file {}", path.display());
                files.push(path);
            }
        }
        children.sort();
        directories.extend(children.into_iter().rev());

        let Some((path, manifest_format)) = preferred_manifest_in_dir(dir, MOON_PKG, MOON_PKG_JSON)
        else {
            continue;
        };
        if emit_warnings
            && matches!(manifest_format, ManifestFormat::New)
            && dir.join(MOON_PKG_JSON).exists()
        {
            warn_known_shadowed_manifest(
                MOON_PKG_JSON,
                MOON_PKG,
                &format!("at package root '{}'", dir.display()),
                user_log,
            );
        }
        let (manifest, supported_targets_decl) = {
            let _span =
                tracing::debug_span!("read_package_manifest", path = %path.display()).entered();
            read_package_desc_file_from_path_with_supported_targets_decl(
                &path,
                user_log,
                allow_legacy,
                emit_warnings,
            )
            .map_err(|inner| DiscoverError::CantReadPackageFile {
                module: source.clone(),
                package: package.clone(),
                path: dir.to_owned(),
                inner,
            })?
        };
        let _sort = tracing::debug_span!("sorting_files").entered();
        files.sort();
        packages.push(PackageInput {
            root_path: dir.to_owned(),
            fqn: PackageFQN::new(source.clone(), package),
            manifest_format,
            manifest,
            supported_targets_decl,
            files,
        });
    }
    Ok(packages)
}
