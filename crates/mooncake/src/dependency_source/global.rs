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

//! Immutable dependency sources stored in the shared global cache.

use std::path::{Path, PathBuf};

use anyhow::Context;
use moonutil::{
    locks::FileLock,
    manifest::read_module_desc_file_in_dir,
    resolution::{DirSyncResult, ModuleSource, ModuleSourceKind, ResolvedEnv},
    toolchain,
    user_log::UserLog,
};

use crate::registry::Registry;

use super::DependencySource;

const LAYOUT_VERSION: &str = "v1";
const SOURCES_DIRECTORY: &str = "sources";

/// The append-only global store for registry source trees.
///
/// This module owns the complete on-disk layout and publication protocol. A
/// caller only asks for the resolved module directories returned by `ensure`.
pub(super) struct ImmutableDependencySource<'a> {
    root: &'a Path,
}

impl<'a> ImmutableDependencySource<'a> {
    pub(super) fn new(root: &'a Path) -> Self {
        Self { root }
    }

    pub(super) fn source_dir(&self, module: &ModuleSource, checksum: &str) -> PathBuf {
        self.root
            .join(LAYOUT_VERSION)
            .join(SOURCES_DIRECTORY)
            .join(module.name().username.as_str())
            .join(module.name().unqual.replace('/', "+"))
            .join(module.version().to_string())
            // Registry versions are intended to be immutable, but keeping the
            // checksum in the physical identity also isolates historical
            // archives that were replaced after publication.
            .join(checksum)
    }

    fn prepare_source(
        &self,
        registry: &dyn Registry,
        module: &ModuleSource,
        directory: &Path,
        frozen: bool,
        user_log: &UserLog,
    ) -> anyhow::Result<()> {
        match std::fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                validate_source(directory, module)
            }
            Ok(_) => anyhow::bail!(
                "Dependency source cache entry is not a directory: `{}`",
                directory.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if frozen {
                    anyhow::bail!(
                        "Failed to sync dependencies: `frozen` is set, so the build system cannot prepare missing shared source {}@{}",
                        module.name(),
                        module.version()
                    );
                }
                let parent = directory.parent().expect("cache entry must have a parent");
                std::fs::create_dir_all(parent)?;
                let staging = tempfile::Builder::new()
                    .prefix(".source-")
                    .tempdir_in(parent)
                    .with_context(|| {
                        format!(
                            "Unable to create staging directory for {}@{}",
                            module.name(),
                            module.version()
                        )
                    })?;
                log::info!(
                    "Preparing immutable package {}@{} at {}",
                    module.name(),
                    module.version(),
                    directory.display()
                );
                registry.extract_to(
                    module.name(),
                    module.version(),
                    staging.path(),
                    !user_log.is_enabled(log::Level::Info),
                )?;
                validate_source(staging.path(), module)?;
                let staging = staging.into_path();
                if let Err(error) = std::fs::rename(&staging, directory) {
                    let _ = std::fs::remove_dir_all(&staging);
                    return Err(error).with_context(|| {
                        format!(
                            "Unable to publish dependency source cache entry `{}`",
                            directory.display()
                        )
                    });
                }
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }
}

impl DependencySource for ImmutableDependencySource<'_> {
    fn ensure(
        &self,
        registry: &dyn Registry,
        resolved: &ResolvedEnv,
        frozen: bool,
        user_log: &UserLog,
    ) -> anyhow::Result<DirSyncResult> {
        let _lock = FileLock::lock_with_user_log(self.root, user_log).with_context(|| {
            format!(
                "Unable to lock dependency source cache `{}`",
                self.root.display()
            )
        })?;
        let mut result = DirSyncResult::default();

        for (id, module) in resolved.all_modules_and_id() {
            let directory = match module.source() {
                ModuleSourceKind::Registry if module.is_core() => toolchain::core(),
                ModuleSourceKind::Registry => {
                    let checksum = registry.source_checksum(module.name(), module.version())?;
                    if checksum.is_empty() || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
                    {
                        anyhow::bail!(
                            "Registry returned an invalid checksum for {}@{}",
                            module.name(),
                            module.version()
                        );
                    }
                    let directory = self.source_dir(module, &checksum);
                    self.prepare_source(registry, module, &directory, frozen, user_log)?;
                    directory
                }
                ModuleSourceKind::Local(path)
                | ModuleSourceKind::Stdlib(path)
                | ModuleSourceKind::SingleFile(path) => path.clone(),
                ModuleSourceKind::Git(url) => {
                    anyhow::bail!("Git dependencies are not supported: {url}")
                }
            };
            result.insert(id, directory);
        }

        Ok(result)
    }
}

fn validate_source(directory: &Path, module: &ModuleSource) -> anyhow::Result<()> {
    let manifest = read_module_desc_file_in_dir(directory)?;
    if manifest.name != module.name().to_string()
        || manifest.version.as_ref() != Some(module.version())
    {
        anyhow::bail!(
            "registry source for {}@{} contains a mismatched module manifest",
            module.name(),
            module.version()
        );
    }
    if manifest
        .scripts
        .as_ref()
        .is_some_and(|scripts| scripts.contains_key("postadd"))
    {
        anyhow::bail!(
            "cannot prepare {}@{} in the shared dependency source cache because it declares scripts.postadd",
            module.name(),
            module.version()
        );
    }
    Ok(())
}
