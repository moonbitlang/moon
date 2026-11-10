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

//! Metadata from the locally synchronized Git index.

mod sync;

use super::RegistryVersionInfo;
use anyhow::Context;
use indexmap::IndexMap;
use moonutil::{
    MoonHomeLayout, dependency::SourceDependencyInfo, registry::RegistryConfig,
    resolution::ModuleName, user_log::UserLog,
};
use once_cell::sync::OnceCell;
use semver::Version;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};
use sync::RegistryIndexUpdate;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RegistryIndexEntry {
    version: String,
    #[serde(default)]
    deps: IndexMap<String, SourceDependencyInfo>,
    checksum: String,
    #[serde(default)]
    yanked: bool,
    #[serde(rename = "yanked_reason")]
    yanked_reason: Option<String>,
}

type ModuleVersions = BTreeMap<Version, RegistryIndexEntry>;
type ModuleVersionsCell = Arc<OnceCell<Arc<ModuleVersions>>>;

pub(super) struct GitIndex {
    home: MoonHomeLayout,
    // The checkout contains the complete index. Retain parsed module files so
    // version and checksum queries use the same records until the next sync.
    parsed_modules: Mutex<HashMap<ModuleName, ModuleVersionsCell>>,
}

impl GitIndex {
    pub(super) fn new(home: MoonHomeLayout) -> Self {
        Self {
            home,
            parsed_modules: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn sync(&self, config: &RegistryConfig, user_log: &UserLog) -> anyhow::Result<()> {
        let outcome = sync::sync(&self.home, config, user_log)?;
        self.parsed_modules.lock().unwrap().clear();
        log_sync_outcome(outcome, user_log);
        Ok(())
    }

    pub(super) fn query_latest_version(
        &self,
        config: &RegistryConfig,
        name: &ModuleName,
        user_log: &UserLog,
    ) -> anyhow::Result<Version> {
        let had_index = self.home.registry_index_dir().exists();
        self.sync(config, user_log).map_err(|error| {
            super::RegistryUnavailable(error.context("Failed to update registry index"))
        })?;
        let version = match self.latest_version(name) {
            Ok(version) => version,
            Err(error) if error.is::<super::RegistryModuleNotFound>() => {
                return Err(error).with_context(|| {
                    if had_index {
                        format!("Module `{name}` not found in registry")
                    } else {
                        format!("Module `{name}` not found in registry after updating the index")
                    }
                });
            }
            Err(error) => return Err(error),
        };
        version.with_context(|| format!("Module `{name}` has no version information"))
    }

    pub(super) fn cached_latest_version(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Option<Version>> {
        match self.latest_version(name) {
            Err(error) if error.is::<super::RegistryModuleNotFound>() => Ok(None),
            result => result,
        }
    }

    fn all_versions_of(&self, name: &ModuleName) -> anyhow::Result<Arc<ModuleVersions>> {
        let parsed = Arc::clone(
            self.parsed_modules
                .lock()
                .unwrap()
                .entry(name.clone())
                .or_default(),
        );
        // Only queries for the same module wait for its file to be parsed.
        // Synchronous and asynchronous callers use this same initialization.
        parsed.get_or_try_init(|| {
            let index_file = self.home.registry_index_file(name);
            tracing::debug!(module = %name, path = %index_file.display(), "reading registry index");
            let file = std::fs::File::open(index_file)
                .map_err(|error| index_read_error(name, error))?;
            parse_versions(name, std::io::BufReader::new(file)).map(Arc::new)
        }).map(Arc::clone)
    }

    pub(super) async fn version_info_async(
        self: &Arc<Self>,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<RegistryVersionInfo> {
        let index = Arc::clone(self);
        let name = name.clone();
        let version = version.clone();
        tokio::task::spawn_blocking(move || {
            index
                .version_info(&name, &version)?
                .ok_or_else(|| super::RegistryVersionNotFound { name, version }.into())
        })
        .await?
    }

    pub(super) fn source_archive_checksum(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<String> {
        self.all_versions_of(name)?
            .get(version)
            .map(|entry| entry.checksum.clone())
            .with_context(|| {
                format!(
                    "No description found for version {} in index file {:?}",
                    version,
                    self.home.registry_index_file(name),
                )
            })
    }

    pub(super) fn version_info(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<Option<RegistryVersionInfo>> {
        Ok(self
            .all_versions_of(name)?
            .get(version)
            .map(|entry| RegistryVersionInfo {
                deps: entry.deps.clone(),
                yanked: entry.yanked,
                yanked_reason: entry.yanked_reason.clone(),
            }))
    }

    fn latest_version(&self, name: &ModuleName) -> anyhow::Result<Option<Version>> {
        Ok(self
            .all_versions_of(name)?
            .last_key_value()
            .map(|(version, _)| version.clone()))
    }
}

fn index_read_error(name: &ModuleName, error: std::io::Error) -> anyhow::Error {
    if error.kind() == std::io::ErrorKind::NotFound {
        super::RegistryModuleNotFound(name.clone()).into()
    } else {
        error.into()
    }
}

fn parse_versions(
    name: &ModuleName,
    reader: impl std::io::BufRead,
) -> anyhow::Result<BTreeMap<Version, RegistryIndexEntry>> {
    let mut versions = BTreeMap::new();
    for line in reader.lines() {
        let line = line?;
        let entry = match serde_json_lenient::from_str::<RegistryIndexEntry>(&line) {
            Ok(entry) => entry,
            Err(error) => {
                log::warn!("Error when reading index file of {}: {}", name, error);
                continue;
            }
        };
        versions.insert(Version::parse(&entry.version)?, entry);
    }
    Ok(versions)
}

fn log_sync_outcome(outcome: RegistryIndexUpdate, user_log: &UserLog) {
    match outcome {
        RegistryIndexUpdate::Cloned => user_log.status("Registry index cloned successfully"),
        RegistryIndexUpdate::Updated => user_log.status("Registry index updated successfully"),
        RegistryIndexUpdate::Recloned => {
            user_log.status("Registry index re-cloned successfully");
        }
        RegistryIndexUpdate::ConcurrentUpdateReused => {
            user_log.status("Registry update already completed by another process");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::LevelFilter;
    use moonutil::user_log::UserLogEntryLevel;

    #[test]
    fn metadata_and_checksum_share_one_parsed_module() {
        for checksum_first in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let home = MoonHomeLayout::new(root.path().to_owned());
            let name: ModuleName = "test/module".into();
            let version = Version::new(1, 0, 0);
            let path = home.registry_index_file(&name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                &path,
                r#"{"version":"1.0.0","deps":{"test/dep":"0.1.0"},"checksum":"original","yanked":true,"yanked_reason":"superseded"}"#,
            )
            .unwrap();
            let index = Arc::new(GitIndex::new(home));
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            if checksum_first {
                assert_eq!(
                    index.source_archive_checksum(&name, &version).unwrap(),
                    "original"
                );
            } else {
                assert!(
                    runtime
                        .block_on(index.version_info_async(&name, &version))
                        .is_ok()
                );
            }

            // A client keeps one observation until it explicitly synchronizes
            // the index, even if another process changes the checkout.
            std::fs::remove_file(path).unwrap();
            assert_eq!(
                index.source_archive_checksum(&name, &version).unwrap(),
                "original"
            );
            let info = runtime
                .block_on(index.version_info_async(&name, &version))
                .unwrap();
            assert!(info.deps.contains_key("test/dep"));
            assert!(info.yanked);
            assert_eq!(info.yanked_reason.as_deref(), Some("superseded"));
        }
    }

    #[test]
    fn registry_update_status_respects_quiet_user_log() {
        let (user_log, capture) = UserLog::captured(LevelFilter::Error);

        log_sync_outcome(RegistryIndexUpdate::Cloned, &user_log);

        assert!(capture.take().is_empty());
    }

    #[test]
    fn registry_update_status_describes_reclone() {
        let (user_log, capture) = UserLog::captured(LevelFilter::Warn);

        log_sync_outcome(RegistryIndexUpdate::Recloned, &user_log);

        let entries = capture.take();
        assert_eq!(entries.len(), 1);
        assert!(
            entries
                .iter()
                .all(|entry| matches!(entry.level, UserLogEntryLevel::Info))
        );
        assert_eq!(
            entries
                .into_iter()
                .map(|entry| entry.message)
                .collect::<Vec<_>>(),
            ["Registry index re-cloned successfully"]
        );
    }

    #[test]
    fn registry_update_status_describes_concurrent_reuse() {
        let (user_log, capture) = UserLog::captured(LevelFilter::Warn);

        log_sync_outcome(RegistryIndexUpdate::ConcurrentUpdateReused, &user_log);

        assert_eq!(
            capture
                .take()
                .into_iter()
                .map(|entry| entry.message)
                .collect::<Vec<_>>(),
            ["Registry update already completed by another process"]
        );
    }
}
