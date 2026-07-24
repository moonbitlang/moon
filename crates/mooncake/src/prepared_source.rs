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

//! Immutable registry sources prepared in Moon's shared dependency cache.
//!
//! This module owns the complete cache-entry lifecycle. Callers receive a
//! source directory paired with the registry checksum that was verified before
//! publication; they do not need to understand locks, staging, or markers.

use std::path::{Path, PathBuf};

use anyhow::Context;
use moonutil::{
    cache::{CacheKind, initialize_cache_root, validate_cache_root},
    locks::FileLock,
    resolution::{DirSyncResult, ModuleId, ModuleSource, ModuleSourceKind, ResolvedEnv},
};
use slotmap::SecondaryMap;

use crate::{dep_dir::non_registry_source_dir, registry::Registry};

const PREPARED_SOURCE_CHECKSUM: &str = "checksum";
const PREPARED_SOURCE_DIR: &str = "source";

pub type PreparedSourceMap = SecondaryMap<ModuleId, PreparedDependencySource>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedDependencySource {
    source_dir: PathBuf,
    verified_checksum: String,
}

impl PreparedDependencySource {
    pub fn source_dir(&self) -> &Path {
        &self.source_dir
    }

    pub fn verified_checksum(&self) -> &str {
        &self.verified_checksum
    }
}

#[derive(Debug, Clone, Default)]
pub struct PreparedDependencySources {
    module_dirs: DirSyncResult,
    registry_sources: PreparedSourceMap,
}

impl PreparedDependencySources {
    pub(crate) fn local(module_dirs: DirSyncResult) -> Self {
        Self {
            module_dirs,
            registry_sources: SecondaryMap::new(),
        }
    }

    pub fn module_dirs(&self) -> &DirSyncResult {
        &self.module_dirs
    }

    pub fn into_module_dirs(self) -> DirSyncResult {
        self.module_dirs
    }

    pub fn into_parts(self) -> (DirSyncResult, PreparedSourceMap) {
        (self.module_dirs, self.registry_sources)
    }
}

#[tracing::instrument(level = "debug", skip_all, fields(cache_root = %cache_root.display()))]
pub(crate) fn prepare_cached_deps(
    cache_root: &Path,
    registry: &dyn Registry,
    pkg_list: &ResolvedEnv,
    quiet: bool,
    frozen: bool,
    verbose: bool,
) -> anyhow::Result<PreparedDependencySources> {
    let has_registry_module = pkg_list
        .all_modules()
        .any(|module| matches!(module.source(), ModuleSourceKind::Registry));
    if has_registry_module {
        if frozen {
            validate_cache_root(CacheKind::DependencySources, cache_root)?;
        } else {
            initialize_cache_root(CacheKind::DependencySources, cache_root)?;
        }
    }

    let mut module_dirs = DirSyncResult::default();
    let mut registry_sources = SecondaryMap::new();
    for (id, module) in pkg_list.all_modules_and_id() {
        match non_registry_source_dir(module) {
            Some(path) => {
                module_dirs.insert(id, path);
            }
            None => {
                let prepared =
                    prepare_cached_dep(cache_root, registry, module, quiet, frozen, verbose)?;
                module_dirs.insert(id, prepared.source_dir.clone());
                registry_sources.insert(id, prepared);
            }
        }
    }
    Ok(PreparedDependencySources {
        module_dirs,
        registry_sources,
    })
}

#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(module = %module.name(), version = %module.version())
)]
fn prepare_cached_dep(
    cache_root: &Path,
    registry: &dyn Registry,
    module: &ModuleSource,
    quiet: bool,
    frozen: bool,
    verbose: bool,
) -> anyhow::Result<PreparedDependencySource> {
    let name = module.name();
    let version = module.version();
    crate::registry::path::validate_module_name(name)?;
    let expected_checksum = registry.checksum_of(name, version)?;
    let module_root = cache_root
        .join("registry")
        .join(name.username.as_str())
        .join(name.unqual.as_str());
    let _lock = if frozen {
        None
    } else {
        std::fs::create_dir_all(&module_root)?;
        Some(FileLock::lock_with_verbosity(&module_root, verbose)?)
    };

    let entry = module_root.join(version.to_string());
    if entry.try_exists()? {
        return open_cached_dep(&entry, name, version, &expected_checksum);
    }

    if frozen {
        anyhow::bail!(
            "Failed to sync dependencies: `frozen` is set, so the build system cannot prepare `{name}@{version}` in the dependency cache"
        );
    }

    let staging = tempfile::TempDir::new_in(&module_root)?;
    let staging_source = staging.path().join(PREPARED_SOURCE_DIR);
    registry.extract_to_verified(name, version, &expected_checksum, &staging_source, quiet)?;
    let manifest = moonutil::manifest::read_module_desc_file_in_dir(&staging_source)?;
    if manifest.name != name.to_string() || manifest.version.as_ref() != Some(version) {
        anyhow::bail!(
            "registry archive for `{name}@{version}` contains manifest for `{}@{}`",
            manifest.name,
            manifest
                .version
                .as_ref()
                .map_or_else(|| "<missing>".to_string(), ToString::to_string)
        );
    }
    if manifest
        .scripts
        .as_ref()
        .is_some_and(|scripts| scripts.contains_key("postadd"))
    {
        anyhow::bail!(
            "dependency `{name}@{version}` declares `scripts.postadd`, which is not supported by the shared dependency cache"
        );
    }
    std::fs::write(
        staging.path().join(PREPARED_SOURCE_CHECKSUM),
        format!("{expected_checksum}\n"),
    )?;
    // The lock keeps readers out until the atomically renamed entry is fully
    // read-only. Some platforms do not allow renaming a read-only directory.
    std::fs::rename(staging.path(), &entry)?;
    if let Err(error) = moonutil::cache::make_cache_tree_readonly(&entry) {
        // Do not leave a visible entry that a later cache hit could accept as
        // a complete publication.
        let _ = moonutil::cache::make_cache_tree_writable(&entry);
        let _ = std::fs::remove_dir_all(&entry);
        return Err(error.into());
    }

    open_cached_dep(&entry, name, version, &expected_checksum)
}

#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(entry = %entry.display(), module = %name, version = %version)
)]
fn open_cached_dep(
    entry: &Path,
    name: &moonutil::resolution::ModuleName,
    version: &semver::Version,
    expected_checksum: &str,
) -> anyhow::Result<PreparedDependencySource> {
    let source = entry.join(PREPARED_SOURCE_DIR);
    let checksum_path = entry.join(PREPARED_SOURCE_CHECKSUM);
    validate_readonly_tree(entry).with_context(|| {
        format!("prepared dependency source `{name}@{version}` has an invalid entry")
    })?;

    let checksum_metadata = std::fs::symlink_metadata(&checksum_path).with_context(|| {
        format!("prepared dependency source `{name}@{version}` is missing checksum metadata")
    })?;
    if !checksum_metadata.is_file() {
        anyhow::bail!(
            "prepared dependency source `{name}@{version}` has invalid checksum metadata"
        );
    }
    let source_metadata = std::fs::symlink_metadata(&source).with_context(|| {
        format!("prepared dependency source `{name}@{version}` is missing its source directory")
    })?;
    if !source_metadata.is_dir() {
        anyhow::bail!(
            "prepared dependency source `{name}@{version}` has an invalid source directory"
        );
    }

    let published_checksum = std::fs::read_to_string(&checksum_path).with_context(|| {
        format!("prepared dependency source `{name}@{version}` is missing checksum metadata")
    })?;
    let published_checksum = published_checksum.trim();
    // A registry version is one immutable identity. A changed checksum is
    // registry corruption, not a new cache variant to publish.
    if published_checksum != expected_checksum {
        anyhow::bail!(
            "registry checksum for `{name}@{version}` changed; published versions are immutable"
        );
    }

    Ok(PreparedDependencySource {
        source_dir: source,
        verified_checksum: published_checksum.to_owned(),
    })
}

fn validate_readonly_tree(root: &Path) -> anyhow::Result<()> {
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        let metadata = std::fs::symlink_metadata(entry.path())?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            anyhow::bail!(
                "prepared dependency source contains symlink `{}`",
                entry.path().display()
            );
        }
        if !metadata.is_dir() && !metadata.is_file() {
            anyhow::bail!(
                "prepared dependency source contains unsupported entry `{}`",
                entry.path().display()
            );
        }
        if !metadata.permissions().readonly() {
            anyhow::bail!(
                "prepared dependency source contains writable entry `{}`",
                entry.path().display()
            );
        }
    }
    Ok(())
}
