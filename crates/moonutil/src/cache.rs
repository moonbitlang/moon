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

use anyhow::{Context, bail};

use crate::{constants::MOON_LOCK, locks::FileLock};

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

    const fn default_directory(self) -> &'static str {
        match self {
            Self::DependencySources => "deps",
            Self::BuildArtifacts => "build",
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

        // Claiming an empty root and writing its ownership marker is one
        // operation. Without the lock, another process can observe the marker
        // after creation but before its contents have been written.
        let _lock = FileLock::lock_with_verbosity(root, false)
            .with_context(|| format!("Unable to lock Moon cache root `{}`", root.display()))?;
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
            path: crate::moon_dir::home()
                .join("cache")
                .join(kind.default_directory()),
        }),
    }
}

fn initialize_ownership_marker(root: &Path, kind: CacheKind) -> anyhow::Result<()> {
    let marker = root.join(OWNERSHIP_MARKER);
    match std::fs::read(&marker) {
        Ok(contents) if contents == kind.ownership() => return Ok(()),
        Ok(_) => {
            bail!(
                "refusing to use Moon cache root `{}` with an incompatible ownership marker",
                root.display()
            )
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    let mut contains_unrecognized_entry = false;
    for entry in std::fs::read_dir(root)? {
        if entry?.file_name() != MOON_LOCK {
            contains_unrecognized_entry = true;
            break;
        }
    }
    if contains_unrecognized_entry {
        bail!(
            "refusing to use unrecognized non-empty Moon cache root `{}`",
            root.display()
        );
    }

    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
    {
        Ok(mut file) => file.write_all(kind.ownership())?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::fs::read(&marker)? != kind.ownership() {
                bail!(
                    "refusing to use Moon cache root `{}` with an incompatible ownership marker",
                    root.display()
                );
            }
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
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
    if metadata.is_dir()
        && matches!(
            std::fs::read(root.join(OWNERSHIP_MARKER)),
            Ok(contents) if contents == kind.ownership()
        )
    {
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
    use super::*;

    #[test]
    fn initializes_ownership_only_for_empty_directory() {
        let parent = tempfile::TempDir::new().unwrap();
        let empty = parent.path().join("empty");
        std::fs::create_dir(&empty).unwrap();
        initialize_ownership_marker(&empty, CacheKind::DependencySources).unwrap();
        assert_eq!(
            std::fs::read(empty.join(OWNERSHIP_MARKER)).unwrap(),
            b"dependency-sources\n"
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
        std::fs::write(root.path().join(OWNERSHIP_MARKER), b"build-artifacts\n").unwrap();

        let error =
            initialize_ownership_marker(root.path(), CacheKind::DependencySources).unwrap_err();
        assert!(error.to_string().contains("incompatible ownership marker"));
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
            b"dependency-sources\n"
        );
    }
}
