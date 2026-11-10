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

//! Persistence for exact HTTP releases and previously observed latest versions.

use super::ModuleMetadata;
use crate::registry::{sha256_hex, stage_registry_cache_file};
use anyhow::{Context, bail};
use moonutil::{
    MoonHomeLayout,
    cache::{CacheKind, CacheRoot, resolve_cache_root_for_home},
    registry::registry_api_url,
    resolution::ModuleName,
};
use once_cell::sync::OnceCell;
use semver::Version;
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// One registry's exact releases and observed latest answers. Memory lookups use
/// module/version keys; only disk access needs the hashed filesystem namespace.
/// Memory locks protect short lookups/publications and never span file or HTTP I/O.
pub(super) struct MetadataCache {
    home: MoonHomeLayout,
    api: String,
    directory: OnceCell<Option<PathBuf>>,
    releases: Mutex<HashMap<ModuleName, HashMap<Version, ModuleMetadata>>>,
    latest: Mutex<HashMap<ModuleName, Version>>,
}

impl MetadataCache {
    pub(super) fn new(home: MoonHomeLayout, api: String) -> Self {
        Self {
            home,
            api,
            directory: OnceCell::new(),
            releases: Mutex::new(HashMap::new()),
            latest: Mutex::new(HashMap::new()),
        }
    }

    fn metadata_cache_dir(&self, name: &ModuleName) -> anyhow::Result<Option<PathBuf>> {
        let directory = self.directory.get_or_try_init(|| -> anyhow::Result<_> {
            let root = resolve_cache_root_for_home(CacheKind::RegistryMetadata, &self.home)?;
            if let CacheRoot::Path { path, .. } = &root {
                create_cache_directory(path)?;
            }
            root.initialize()?;
            Ok(match root {
                CacheRoot::Disabled => None,
                CacheRoot::Path { path, .. } => Some(
                    path.join("v1")
                        .join(sha256_hex(registry_api_url(&self.api)?.as_str().as_bytes())),
                ),
            })
        })?;
        Ok(directory
            .as_ref()
            .map(|root| root.join(sha256_hex(name.to_string().as_bytes()))))
    }

    fn latest_cache_path(&self, name: &ModuleName) -> anyhow::Result<Option<PathBuf>> {
        Ok(self
            .metadata_cache_dir(name)?
            .map(|path| path.join("latest.json")))
    }

    fn release_cache_path(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<Option<PathBuf>> {
        Ok(self.metadata_cache_dir(name)?.map(|path| {
            path.join("releases").join(format!(
                "{}.json",
                sha256_hex(version.to_string().as_bytes())
            ))
        }))
    }

    pub(super) fn release(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<Option<ModuleMetadata>> {
        if let Some(cached) = self
            .releases
            .lock()
            .unwrap()
            .get(name)
            .and_then(|versions| versions.get(version))
        {
            return Ok(Some(cached.clone()));
        }
        let Some(path) = self.release_cache_path(name, version)? else {
            return Ok(None);
        };
        let Some(cached) = Self::read_release(&path, name, version)? else {
            return Ok(None);
        };
        // A concurrent store may have published a newer observation while we
        // read the file. Keep that observation instead of overwriting it.
        let mut releases = self.releases.lock().unwrap();
        let cached = releases
            .entry(name.clone())
            .or_default()
            .entry(version.clone())
            .or_insert(cached);
        Ok(Some(cached.clone()))
    }

    fn read_release(
        path: &Path,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<Option<ModuleMetadata>> {
        let cached = match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice::<ModuleMetadata>(&bytes)
                .context("failed to parse cached registry release")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("failed to read cached registry release"),
        };
        cached.validate(name, Some(version))?;
        Ok(Some(cached))
    }

    pub(super) fn latest(&self, name: &ModuleName) -> anyhow::Result<Option<Version>> {
        if let Some(version) = self.latest.lock().unwrap().get(name) {
            return Ok(Some(version.clone()));
        }
        let Some(path) = self.latest_cache_path(name)? else {
            return Ok(None);
        };
        let version = match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice::<Version>(&bytes)
                .context("failed to parse cached latest version")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("failed to read cached latest version"),
        };
        if self.release(name, &version)?.is_none() {
            return Ok(None);
        }
        self.latest
            .lock()
            .unwrap()
            .insert(name.clone(), version.clone());
        Ok(Some(version))
    }

    fn write_metadata_cache<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
        let parent = path.parent().expect("metadata file has a parent");
        create_cache_directory(parent)?;
        let (mut staged, path) = stage_registry_cache_file(path)?;
        serde_json::to_writer(staged.as_file_mut(), value)?;
        staged.as_file_mut().sync_all()?;
        staged.persist(path).map_err(|error| error.error)?;
        Ok(())
    }

    /// Serialize check-and-publish across processes. Atomic replacement alone
    /// would let concurrent, conflicting responses overwrite one another.
    pub(super) fn store_release(
        &self,
        name: &ModuleName,
        metadata: ModuleMetadata,
    ) -> anyhow::Result<ModuleMetadata> {
        let Some(path) = self.release_cache_path(name, &metadata.version)? else {
            // Without a disk cache, compare and publish under the memory lock.
            let mut releases = self.releases.lock().unwrap();
            let versions = releases.entry(name.clone()).or_default();
            let cached = versions.get(&metadata.version).cloned();
            let metadata = Self::merge_release(name, metadata, cached)?;
            versions.insert(metadata.version.clone(), metadata.clone());
            return Ok(metadata);
        };
        let parent = path.parent().expect("release cache has a parent");
        create_cache_directory(parent)?;
        let lock_path = std::fs::canonicalize(parent)?
            .join(path.file_name().unwrap())
            .with_extension("json.lock");
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        lock.lock()?;
        // Keep the file lock through both disk and memory publication, so two
        // writers cannot leave memory with an older advisory than the file.
        let cached = Self::read_release(&path, name, &metadata.version)?;
        let metadata = Self::merge_release(name, metadata, cached)?;
        Self::write_metadata_cache(&path, &metadata)?;
        self.releases
            .lock()
            .unwrap()
            .entry(name.clone())
            .or_default()
            .insert(metadata.version.clone(), metadata.clone());
        Ok(metadata)
    }

    fn merge_release(
        name: &ModuleName,
        mut metadata: ModuleMetadata,
        cached: Option<ModuleMetadata>,
    ) -> anyhow::Result<ModuleMetadata> {
        if let Some(mut cached) = cached {
            // Compare dependency meaning, not JSON spelling or map order. Both
            // records have already been validated as exact registry dependencies.
            let same_dependencies = cached.deps.len() == metadata.deps.len()
                && cached.deps.iter().all(|(name, dependency)| {
                    metadata
                        .deps
                        .get(name)
                        .is_some_and(|incoming| incoming.version() == dependency.version())
                });
            if cached.visibility != metadata.visibility
                || !same_dependencies
                || !cached.checksum.eq_ignore_ascii_case(&metadata.checksum)
            {
                bail!(
                    "registry changed immutable metadata for {name}@{}; cached release preserved",
                    metadata.version
                );
            }
            // Advisory status is mutable; dependency and archive identity is not.
            cached.yanked = metadata.yanked;
            cached.yanked_reason = metadata.yanked_reason;
            metadata = cached;
        }
        Ok(metadata)
    }

    pub(super) fn store_latest(&self, name: &ModuleName, version: &Version) -> anyhow::Result<()> {
        if let Some(path) = self.latest_cache_path(name)? {
            Self::write_metadata_cache(&path, version)?;
        }
        self.latest
            .lock()
            .unwrap()
            .insert(name.clone(), version.clone());
        Ok(())
    }
}

fn create_cache_directory(parent: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        // Apply owner-only permissions to every newly created parent.
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(parent)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::http::ModuleVisibility;
    use indexmap::IndexMap;

    #[test]
    fn release_cache_supports_long_paths_and_replacement() {
        let sandbox = tempfile::tempdir().unwrap();
        let home = MoonHomeLayout::new(sandbox.path().join("metadata-cache-home/".repeat(18)));
        assert!(home.root().to_string_lossy().len() > 260);
        let api = "https://example.com";
        let registry = MetadataCache::new(home.clone(), api.to_owned());
        let name: ModuleName = "alice/example".into();
        let version = Version::new(0, 1, 0);
        let metadata = ModuleMetadata {
            module: name.to_string(),
            visibility: ModuleVisibility::Public,
            version: version.clone(),
            checksum: "b".repeat(64),
            deps: IndexMap::new(),
            yanked: false,
            yanked_reason: None,
        };
        // HTTP responses may encode an empty dependency map as null or an object.
        for deps in [serde_json::Value::Null, serde_json::json!({})] {
            let mut record = serde_json::to_value(&metadata).unwrap();
            record["deps"] = deps;
            MetadataCache::write_metadata_cache(
                &registry
                    .release_cache_path(&name, &version)
                    .unwrap()
                    .unwrap(),
                &record,
            )
            .unwrap();
            MetadataCache::write_metadata_cache(
                &registry.latest_cache_path(&name).unwrap().unwrap(),
                &version,
            )
            .unwrap();
            registry.store_release(&name, metadata.clone()).unwrap();
            let fresh = MetadataCache::new(home.clone(), api.to_owned());
            assert!(
                fresh
                    .release(&name, &version)
                    .unwrap()
                    .unwrap()
                    .deps
                    .is_empty()
            );
            assert_eq!(
                fresh.release(&name, &version).unwrap().unwrap().checksum,
                "b".repeat(64)
            );
            assert_eq!(fresh.latest(&name).unwrap(), Some(version.clone()));
        }
    }

    #[test]
    fn conflicting_concurrent_releases_preserve_the_first_published_record() {
        let sandbox = tempfile::tempdir().unwrap();
        let home = MoonHomeLayout::new(sandbox.path().to_owned());
        let api = "https://example.com";
        let name: ModuleName = "alice/example".into();
        let barrier = std::sync::Barrier::new(2);
        let outcomes = std::thread::scope(|scope| {
            let threads = ["a", "b"].map(|hex| {
                let home = home.clone();
                let name = &name;
                let barrier = &barrier;
                scope.spawn(move || {
                    let cache = MetadataCache::new(home, api.to_owned());
                    let metadata = ModuleMetadata {
                        module: name.to_string(),
                        visibility: ModuleVisibility::Public,
                        version: Version::new(0, 1, 0),
                        checksum: hex.repeat(64),
                        deps: IndexMap::new(),
                        yanked: false,
                        yanked_reason: None,
                    };
                    barrier.wait();
                    cache.store_release(name, metadata)
                })
            });
            threads.map(|thread| thread.join().unwrap())
        });
        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
        assert!(
            outcomes
                .iter()
                .find_map(|result| result.as_ref().err())
                .unwrap()
                .to_string()
                .contains("immutable")
        );
        let published = MetadataCache::new(home, api.to_owned())
            .release(&name, &Version::new(0, 1, 0))
            .unwrap()
            .unwrap();
        assert_eq!(
            published.checksum,
            outcomes.into_iter().find_map(Result::ok).unwrap().checksum
        );
    }
}
