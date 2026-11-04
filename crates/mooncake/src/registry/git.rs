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

use super::RegistryVersionInfo;
use crate::update::{RegistryIndexRecloneReason, RegistryIndexUpdate, UpdateOutcome};
use anyhow::{Context, bail};
use indexmap::IndexMap;
use moonutil::{
    MoonHomeLayout, dependency::SourceDependencyInfo, registry::RegistryConfig,
    resolution::ModuleName, user_log::UserLog,
};
use semver::Version;
use serde::Deserialize;
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    io::BufRead,
    sync::Arc,
};

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

pub(super) struct GitIndex {
    home: MoonHomeLayout,
    cache: RefCell<HashMap<ModuleName, Arc<BTreeMap<Version, RegistryVersionInfo>>>>,
}

impl GitIndex {
    pub(super) fn new(home: MoonHomeLayout) -> Self {
        Self {
            home,
            cache: RefCell::new(HashMap::new()),
        }
    }

    pub(super) fn sync(&self, config: &RegistryConfig, user_log: &UserLog) -> anyhow::Result<()> {
        let outcome = crate::update::sync(&self.home, config, user_log)?;
        self.cache.borrow_mut().clear();
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

    /// Only absence of the entire module permits another metadata transport.
    /// Empty entries, missing versions and read errors must not become fallbacks.
    pub(super) fn contains_module(&self, name: &ModuleName) -> anyhow::Result<bool> {
        match self.all_versions_of(name) {
            Ok(_) => Ok(true),
            Err(error) if error.is::<super::RegistryModuleNotFound>() => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn all_versions_of(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Arc<BTreeMap<Version, RegistryVersionInfo>>> {
        // check cache
        if let Some(v) = self.cache.borrow().get(name) {
            return Ok(Arc::clone(v));
        }

        let index_file = self.home.registry_index_file(name);
        log::debug!("Reading versions of {} from {}", name, index_file.display());
        let file = std::fs::File::open(index_file).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                anyhow::Error::new(super::RegistryModuleNotFound(name.clone()))
            } else {
                error.into()
            }
        })?;
        let reader = std::io::BufReader::new(file);

        let lines = reader.lines();
        let mut res = BTreeMap::new();
        for line in lines {
            let line = line?;
            let entry = match serde_json_lenient::from_str::<RegistryIndexEntry>(&line) {
                Ok(m) => m,
                Err(e) => {
                    log::warn!("Error when reading index file of {}: {}", name, e);
                    continue;
                }
            };
            res.insert(
                Version::parse(&entry.version)?,
                RegistryVersionInfo {
                    deps: entry.deps,
                    yanked: entry.yanked,
                    yanked_reason: entry.yanked_reason,
                },
            );
        }

        // put in cache
        let res = Arc::new(res);
        self.cache
            .borrow_mut()
            .insert(name.clone(), Arc::clone(&res));

        Ok(res)
    }

    pub(super) fn source_archive_checksum(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<String> {
        let p = self.home.registry_index_file(name);
        let file = std::fs::File::open(&p)?;
        let reader = std::io::BufReader::new(file);

        let lines = reader.lines().collect::<std::io::Result<Vec<String>>>()?;
        let version_str = version.to_string();
        for line in lines.iter().rev() {
            let entry = serde_json_lenient::from_str::<RegistryIndexEntry>(line)?;
            if entry.version == version_str {
                return Ok(entry.checksum);
            }
        }
        bail!(
            "No description found for version {} in index file {:?}",
            version,
            p,
        );
    }

    pub(super) fn version_info(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<Option<RegistryVersionInfo>> {
        Ok(self.all_versions_of(name)?.get(version).cloned())
    }

    fn latest_version(&self, name: &ModuleName) -> anyhow::Result<Option<Version>> {
        Ok(self
            .all_versions_of(name)?
            .last_key_value()
            .map(|(version, _)| version.clone()))
    }
}

fn log_sync_outcome(outcome: UpdateOutcome, user_log: &UserLog) {
    match outcome.registry_index {
        RegistryIndexUpdate::Cloned => user_log.status("Registry index cloned successfully"),
        RegistryIndexUpdate::Updated => user_log.status("Registry index updated successfully"),
        RegistryIndexUpdate::Recloned(reason) => {
            let reason = match reason {
                RegistryIndexRecloneReason::UpdateFailed => "Failed to update registry index",
                RegistryIndexRecloneReason::RemoteMismatch => {
                    "Registry index remote does not match the configured URL"
                }
                RegistryIndexRecloneReason::NotGitRepository => {
                    "Registry index is not a Git repository"
                }
                RegistryIndexRecloneReason::MissingOrigin => "Registry index has no origin remote",
            };
            user_log.status(format!("{reason}, re-cloning"));
            user_log.status("Registry index re-cloned successfully");
        }
        RegistryIndexUpdate::ConcurrentUpdateReused => {
            user_log.status("Registry update already completed by another process");
            return;
        }
    }
    if outcome.symbols_updated {
        user_log.status("Symbols updated successfully");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::LevelFilter;
    use moonutil::user_log::UserLogEntryLevel;

    #[test]
    fn registry_update_status_respects_quiet_user_log() {
        let (user_log, capture) = UserLog::captured(LevelFilter::Error);

        log_sync_outcome(
            UpdateOutcome {
                registry_index: RegistryIndexUpdate::Cloned,
                symbols_updated: true,
            },
            &user_log,
        );

        assert!(capture.take().is_empty());
    }

    #[test]
    fn registry_update_status_describes_reclone() {
        let (user_log, capture) = UserLog::captured(LevelFilter::Warn);

        log_sync_outcome(
            UpdateOutcome {
                registry_index: RegistryIndexUpdate::Recloned(
                    RegistryIndexRecloneReason::NotGitRepository,
                ),
                symbols_updated: true,
            },
            &user_log,
        );

        let entries = capture.take();
        assert_eq!(entries.len(), 3);
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
            [
                "Registry index is not a Git repository, re-cloning",
                "Registry index re-cloned successfully",
                "Symbols updated successfully",
            ]
        );
    }

    #[test]
    fn registry_update_status_describes_concurrent_reuse() {
        let (user_log, capture) = UserLog::captured(LevelFilter::Warn);

        log_sync_outcome(
            UpdateOutcome {
                registry_index: RegistryIndexUpdate::ConcurrentUpdateReused,
                symbols_updated: true,
            },
            &user_log,
        );

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
