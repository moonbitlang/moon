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
    fs::OpenOptions,
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use fs4::fs_std::FileExt;

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
    Path(PathBuf),
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
            Ok(CacheRoot::Path(path))
        }
        None => Ok(CacheRoot::Path(
            crate::moon_dir::home()
                .join("cache")
                .join(kind.default_directory()),
        )),
    }
}

/// Claim an empty cache root for Moon or validate its existing ownership.
///
/// Cache writers call this before creating implementation-specific entries.
/// Refusing a non-empty unowned directory keeps `moon clean` from later
/// treating unrelated user data as Moon-owned state.
pub fn initialize_cache_root(kind: CacheKind, root: &Path) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!(
                "refusing to use symlinked Moon cache root `{}`",
                root.display()
            )
        }
        Ok(metadata) if !metadata.is_dir() => {
            bail!("Moon cache root is not a directory: `{}`", root.display())
        }
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {
            std::fs::create_dir_all(root).with_context(|| {
                format!("failed to create Moon cache root `{}`", root.display())
            })?;
        }
        Err(error) => return Err(error.into()),
    }

    let marker = root.join(OWNERSHIP_MARKER);
    if !marker.exists() && has_non_marker_entry(root, &marker)? {
        bail!(
            "refusing to use non-empty unrecognized Moon cache root `{}`",
            root.display()
        );
    }

    let mut marker_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&marker)
        .with_context(|| format!("failed to open Moon cache marker `{}`", marker.display()))?;
    marker_file
        .lock_exclusive()
        .with_context(|| format!("failed to lock Moon cache marker `{}`", marker.display()))?;

    let mut contents = Vec::new();
    marker_file.read_to_end(&mut contents)?;
    if contents == kind.ownership() {
        return Ok(());
    }
    if !contents.is_empty() {
        bail!(
            "Moon cache root has incompatible ownership: `{}`",
            root.display()
        );
    }

    if has_non_marker_entry(root, &marker)? {
        bail!(
            "refusing to use non-empty unrecognized Moon cache root `{}`",
            root.display()
        );
    }
    marker_file.write_all(kind.ownership())?;
    marker_file.sync_all()?;
    Ok(())
}

fn has_non_marker_entry(root: &Path, marker: &Path) -> std::io::Result<bool> {
    for entry in std::fs::read_dir(root)? {
        if entry?.path() != marker {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn clean_cache(kind: CacheKind) -> anyhow::Result<()> {
    let CacheRoot::Path(root) = resolve_cache_root(kind)? else {
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
    use std::sync::{Arc, Barrier};

    use super::*;

    #[test]
    fn initializes_empty_cache_root_and_reuses_it() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("build-cache");

        initialize_cache_root(CacheKind::BuildArtifacts, &root).unwrap();
        initialize_cache_root(CacheKind::BuildArtifacts, &root).unwrap();

        assert_eq!(
            std::fs::read(root.join(OWNERSHIP_MARKER)).unwrap(),
            b"build-artifacts\n"
        );
    }

    #[test]
    fn refuses_to_claim_non_empty_unowned_root() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("user-data"), "keep").unwrap();

        assert!(
            initialize_cache_root(CacheKind::BuildArtifacts, root.path())
                .unwrap_err()
                .to_string()
                .contains("non-empty unrecognized")
        );
    }

    #[test]
    fn concurrent_initializers_accept_the_same_owner() {
        let parent = tempfile::tempdir().unwrap();
        let root = Arc::new(parent.path().join("build-cache"));
        let barrier = Arc::new(Barrier::new(8));
        let initializers = (0..8)
            .map(|_| {
                let root = Arc::clone(&root);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    initialize_cache_root(CacheKind::BuildArtifacts, &root)
                })
            })
            .collect::<Vec<_>>();

        for initializer in initializers {
            initializer.join().unwrap().unwrap();
        }
    }
}
