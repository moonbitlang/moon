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

//! Immutable dependency sources stored in the shared global cache.

use std::path::{Path, PathBuf};

use anyhow::Context;
use moonutil::{
    locks::lock_directory,
    manifest::{MoonMod, read_dependency_module_manifest},
    resolution::{DirSyncResult, ModuleDependencyGraph, ModuleSource, ModuleSourceKind},
    toolchain,
    user_log::UserLog,
};

use crate::{
    pkg::legacy_postadd,
    registry::{RegistrySource, SourceRequest},
    zip_util::extract_archives,
};

use super::DependencySource;

const LAYOUT_VERSION: &str = "v1";
const SOURCES_DIRECTORY: &str = "sources";
pub(super) const SOURCE_ARCHIVE_CHECKSUM_FILE: &str = ".moon-source-archive-checksum";

/// The append-only global store for registry source trees.
///
/// This module owns the complete on-disk layout and publication protocol. A
/// caller only asks for the resolved module directories returned by `ensure`.
pub(super) struct ImmutableDependencySource<'a> {
    root: &'a Path,
    jobs: Option<usize>,
}

impl<'a> ImmutableDependencySource<'a> {
    pub(super) fn new(root: &'a Path, jobs: Option<usize>) -> Self {
        Self { root, jobs }
    }

    pub(super) fn source_dir(&self, module: &ModuleSource) -> PathBuf {
        self.root
            .join(LAYOUT_VERSION)
            .join(SOURCES_DIRECTORY)
            .join(module.name().username.as_str())
            .join(module.name().unqual.replace('/', "+"))
            .join(module.version().to_string())
    }

    fn stage_source(
        &self,
        module: &ModuleSource,
        checksum: &str,
        directory: &Path,
        frozen: bool,
    ) -> anyhow::Result<Option<tempfile::TempDir>> {
        match std::fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                validate_cached_source(directory, module, checksum)?;
                Ok(None)
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
                Ok(Some(staging))
            }
            Err(error) => Err(error.into()),
        }
    }
}

impl DependencySource for ImmutableDependencySource<'_> {
    fn ensure(
        &self,
        registry: &dyn RegistrySource,
        resolved: &ModuleDependencyGraph,
        frozen: bool,
        user_log: &UserLog,
    ) -> anyhow::Result<DirSyncResult> {
        let _lock = lock_directory(self.root, user_log).with_context(|| {
            format!(
                "Unable to lock dependency source cache `{}`",
                self.root.display()
            )
        })?;
        let mut result = DirSyncResult::default();
        let mut staged = Vec::new();

        for (id, module) in resolved.all_modules_and_id() {
            let directory = match module.source() {
                ModuleSourceKind::Registry if module.is_core() => toolchain::core(),
                ModuleSourceKind::Registry => {
                    let checksum = registry.source_archive_checksum(module)?;
                    if checksum.len() != 64
                        || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
                    {
                        anyhow::bail!(
                            "Registry returned an invalid source archive checksum for {}@{}",
                            module.name(),
                            module.version()
                        );
                    }
                    let directory = self.source_dir(module);
                    if let Some(staging) =
                        self.stage_source(module, &checksum, &directory, frozen)?
                    {
                        staged.push((module, checksum, directory.clone(), staging));
                    }
                    directory
                }
                ModuleSourceKind::Local(path)
                | ModuleSourceKind::Stdlib(path)
                | ModuleSourceKind::SingleFile(path) => path.clone(),
            };
            result.insert(id, directory);
        }

        let sources = staged
            .iter()
            .map(|(module, checksum, _, _)| SourceRequest {
                module: (*module).clone(),
                checksum: checksum.clone(),
            })
            .collect::<Vec<_>>();
        let archives = registry.download_archives(&sources, user_log)?;
        extract_archives(
            archives
                .into_iter()
                .zip(&staged)
                .map(|(archive, (_, _, _, staging))| (archive, staging.path().to_owned()))
                .collect(),
            self.jobs,
        )?;
        for (module, checksum, directory, staging) in staged {
            let manifest = validate_source(staging.path(), module)?;
            legacy_postadd::warn_skipped(&manifest, user_log);
            std::fs::write(staging.path().join(SOURCE_ARCHIVE_CHECKSUM_FILE), checksum)?;
            std::fs::rename(staging.path(), &directory).with_context(|| {
                format!(
                    "Unable to publish dependency source cache entry `{}`",
                    directory.display()
                )
            })?;
        }

        Ok(result)
    }
}

fn validate_cached_source(
    directory: &Path,
    module: &ModuleSource,
    expected_checksum: &str,
) -> anyhow::Result<()> {
    let checksum_file = directory.join(SOURCE_ARCHIVE_CHECKSUM_FILE);
    let cached_checksum = match std::fs::read_to_string(&checksum_file) {
        Ok(checksum) => checksum,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!(
                "dependency source cache entry for {}@{} is missing archive checksum metadata; run `moon clean --dep-cache` before retrying",
                module.name(),
                module.version()
            )
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "Unable to read dependency source cache metadata `{}`",
                    checksum_file.display()
                )
            });
        }
    };
    if cached_checksum.len() != 64 || !cached_checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        anyhow::bail!(
            "dependency source cache entry for {}@{} has invalid archive checksum metadata; run `moon clean --dep-cache` before retrying",
            module.name(),
            module.version()
        );
    }
    if cached_checksum != expected_checksum {
        anyhow::bail!(
            "registry archive checksum changed for {}@{}; run `moon clean --dep-cache` before retrying",
            module.name(),
            module.version()
        );
    }
    validate_source(directory, module).map(|_| ())
}

fn validate_source(directory: &Path, module: &ModuleSource) -> anyhow::Result<MoonMod> {
    let manifest = read_dependency_module_manifest(directory)
        .with_context(|| format!("Failed to load dependency `{module}`"))?;
    if manifest.name != module.name().to_string()
        || manifest.version.as_ref() != Some(module.version())
    {
        anyhow::bail!(
            "registry source for {}@{} contains a mismatched module manifest",
            module.name(),
            module.version()
        );
    }
    Ok(manifest)
}
