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

//! Configuration and lifecycle of Moon-owned global cache roots.
//!
//! Cache contents are intentionally opaque here. Source and artifact stores
//! may choose their own representations without changing the CLI contract.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::bail;

const OWNERSHIP_MARKER: &str = ".moon-cache";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheKind {
    DependencySources,
    BuildArtifacts,
}

impl CacheKind {
    pub const fn environment_variable(self) -> &'static str {
        match self {
            Self::DependencySources => "MOON_DEP_CACHE",
            Self::BuildArtifacts => "MOON_BUILD_CACHE",
        }
    }

    const fn ownership(self) -> &'static [u8] {
        match self {
            Self::DependencySources => b"dependency-sources\n",
            Self::BuildArtifacts => b"build-artifacts\n",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheRoot {
    Disabled,
    Path { kind: CacheKind, path: PathBuf },
}

impl CacheRoot {
    /// Claim this configured root for one Moon cache kind before writing data.
    ///
    /// Disabled roots return `None`. Enabled roots are created if necessary and
    /// accepted only when empty or already owned by the requested cache kind.
    pub fn initialize(&self) -> anyhow::Result<Option<&Path>> {
        let Self::Path { kind, path: root } = self else {
            return Ok(None);
        };

        match std::fs::symlink_metadata(root) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "refusing to use symlinked Moon cache root `{}`",
                    root.display()
                )
            }
            Ok(metadata) if !metadata.is_dir() => {
                bail!(
                    "refusing to use non-directory Moon cache root `{}`",
                    root.display()
                )
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir_all(root)?;
            }
            Err(error) => return Err(error.into()),
        }

        initialize_ownership_marker(root, *kind)?;
        Ok(Some(root))
    }
}

pub fn resolve_cache_root(kind: CacheKind) -> anyhow::Result<CacheRoot> {
    let environment = kind.environment_variable();
    match std::env::var_os(environment) {
        Some(value) if value == "off" => Ok(CacheRoot::Disabled),
        Some(value) => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                bail!("{environment} must be an absolute path or `off`");
            }
            Ok(CacheRoot::Path { kind, path })
        }
        None => Ok(CacheRoot::Path {
            kind,
            path: match kind {
                CacheKind::DependencySources => crate::MOON_HOME.dependency_cache_dir(),
                CacheKind::BuildArtifacts => crate::MOON_HOME.build_cache_dir(),
            },
        }),
    }
}

fn initialize_ownership_marker(root: &Path, kind: CacheKind) -> anyhow::Result<()> {
    if validate_existing_ownership_marker(root, kind)? {
        return Ok(());
    }

    if std::fs::read_dir(root)?.next().transpose()?.is_some() {
        // Another initializer may have published the marker after our first
        // lookup. Recheck before classifying the root as unowned.
        if validate_existing_ownership_marker(root, kind)? {
            return Ok(());
        }
        bail!(
            "refusing to use unrecognized non-empty Moon cache root `{}`",
            root.display()
        );
    }

    // Build the complete marker next to the root, then publish it without
    // replacing an existing claim. Keeping the temporary file outside `root`
    // means another initializer never mistakes our staging file for user data.
    let parent = root.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "Moon cache root `{}` must have a parent directory",
            root.display()
        )
    })?;
    let mut marker = tempfile::NamedTempFile::new_in(parent)?;
    marker.write_all(kind.ownership())?;
    marker.flush()?;
    match marker.persist_noclobber(root.join(OWNERSHIP_MARKER)) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            if !validate_existing_ownership_marker(root, kind)? {
                bail!(
                    "Moon cache ownership marker disappeared while initializing `{}`",
                    root.display()
                );
            }
            Ok(())
        }
        Err(error) => Err(error.error.into()),
    }
}

fn validate_existing_ownership_marker(root: &Path, kind: CacheKind) -> anyhow::Result<bool> {
    let marker = root.join(OWNERSHIP_MARKER);
    match std::fs::symlink_metadata(&marker) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            if std::fs::read(marker)? == kind.ownership() {
                Ok(true)
            } else {
                bail!(
                    "refusing to use Moon cache root `{}` with an incompatible ownership marker",
                    root.display()
                )
            }
        }
        Ok(_) => {
            bail!(
                "refusing to use Moon cache root `{}` with an incompatible ownership marker",
                root.display()
            )
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub fn clean_cache(kind: CacheKind) -> anyhow::Result<()> {
    let CacheRoot::Path { path: root, .. } = resolve_cache_root(kind)? else {
        return Ok(());
    };
    let metadata = match std::fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        bail!(
            "refusing to clean symlinked Moon cache root `{}`",
            root.display()
        );
    }
    if metadata.is_dir() && std::fs::read_dir(&root)?.next().transpose()?.is_none() {
        std::fs::remove_dir(root)?;
        return Ok(());
    }
    if metadata.is_dir() && validate_existing_ownership_marker(&root, kind)? {
        std::fs::remove_dir_all(root)?;
        return Ok(());
    }
    bail!(
        "refusing to clean unrecognized Moon cache root `{}`",
        root.display()
    )
}

#[cfg(test)]
mod tests {
    use crate::constants::MOON_LOCK;

    use super::*;

    #[test]
    fn initializes_ownership_only_for_empty_directory() {
        let parent = tempfile::TempDir::new().unwrap();
        let empty = parent.path().join("empty");
        std::fs::create_dir(&empty).unwrap();
        initialize_ownership_marker(&empty, CacheKind::DependencySources).unwrap();
        assert_eq!(
            std::fs::read(empty.join(OWNERSHIP_MARKER)).unwrap(),
            CacheKind::DependencySources.ownership()
        );

        let unowned = parent.path().join("unowned");
        std::fs::create_dir(&unowned).unwrap();
        std::fs::write(unowned.join("user-data"), "keep").unwrap();
        let error =
            initialize_ownership_marker(&unowned, CacheKind::DependencySources).unwrap_err();
        assert!(error.to_string().contains("unrecognized non-empty"));
        assert_eq!(
            std::fs::read_to_string(unowned.join("user-data")).unwrap(),
            "keep"
        );
        assert!(!unowned.join(OWNERSHIP_MARKER).exists());
    }

    #[test]
    fn rejects_incompatible_ownership_marker() {
        let root = tempfile::TempDir::new().unwrap();
        std::fs::write(
            root.path().join(OWNERSHIP_MARKER),
            CacheKind::BuildArtifacts.ownership(),
        )
        .unwrap();

        let error =
            initialize_ownership_marker(root.path(), CacheKind::DependencySources).unwrap_err();
        assert!(error.to_string().contains("incompatible ownership marker"));
    }

    #[test]
    fn initialize_does_not_modify_an_unowned_root() {
        let root = tempfile::TempDir::new().unwrap();
        std::fs::write(root.path().join("user-data"), "keep").unwrap();
        std::fs::write(root.path().join(MOON_LOCK), "unrelated lock contents").unwrap();
        let cache = CacheRoot::Path {
            kind: CacheKind::DependencySources,
            path: root.path().to_path_buf(),
        };

        let error = cache.initialize().unwrap_err();

        assert!(error.to_string().contains("unrecognized non-empty"));
        assert_eq!(
            std::fs::read_to_string(root.path().join("user-data")).unwrap(),
            "keep"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join(MOON_LOCK)).unwrap(),
            "unrelated lock contents"
        );
        assert!(!root.path().join(OWNERSHIP_MARKER).exists());
    }

    #[test]
    fn initializes_ownership_concurrently() {
        let parent = tempfile::TempDir::new().unwrap();
        let root = CacheRoot::Path {
            kind: CacheKind::DependencySources,
            path: parent.path().join("cache"),
        };

        std::thread::scope(|scope| {
            let first = scope.spawn(|| root.initialize().unwrap());
            let second = scope.spawn(|| root.initialize().unwrap());
            first.join().unwrap();
            second.join().unwrap();
        });

        let CacheRoot::Path { path, .. } = root else {
            unreachable!()
        };
        assert_eq!(
            std::fs::read(path.join(OWNERSHIP_MARKER)).unwrap(),
            CacheKind::DependencySources.ownership()
        );
    }

    #[test]
    fn rejects_malformed_ownership_marker() {
        let root = tempfile::TempDir::new().unwrap();
        std::fs::write(root.path().join(OWNERSHIP_MARKER), "user data").unwrap();

        let error =
            initialize_ownership_marker(root.path(), CacheKind::DependencySources).unwrap_err();

        assert!(error.to_string().contains("incompatible ownership marker"));
        assert_eq!(
            std::fs::read_to_string(root.path().join(OWNERSHIP_MARKER)).unwrap(),
            "user data"
        );
    }

    #[test]
    fn different_cache_kinds_cannot_claim_the_same_root() {
        let parent = tempfile::TempDir::new().unwrap();
        let root = parent.path().join("cache");
        std::fs::create_dir(&root).unwrap();

        let (dependency_result, build_result) = std::thread::scope(|scope| {
            let dependency =
                scope.spawn(|| initialize_ownership_marker(&root, CacheKind::DependencySources));
            let build =
                scope.spawn(|| initialize_ownership_marker(&root, CacheKind::BuildArtifacts));
            (dependency.join().unwrap(), build.join().unwrap())
        });

        assert_ne!(dependency_result.is_ok(), build_result.is_ok());
        let ownership = std::fs::read(root.join(OWNERSHIP_MARKER)).unwrap();
        assert!(
            ownership == CacheKind::DependencySources.ownership()
                || ownership == CacheKind::BuildArtifacts.ownership()
        );
    }
}
